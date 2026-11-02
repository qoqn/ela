use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Notify;

use crate::wire;

/// Beyond this, painting many small pieces costs more than sending their bounding box.
const MAX_DIRTY_RECTS: usize = 16;
/// A rect is large enough to send lossy when it covers at least 1/this of the surface.
const LOSSY_SHARE: u64 = 4;

pub(crate) struct TakenFrame {
    pub message: Vec<u8>,
    pub lossy: Vec<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }

    fn right(self) -> u32 {
        u32::from(self.x) + u32::from(self.w)
    }

    fn bottom(self) -> u32 {
        u32::from(self.y) + u32::from(self.h)
    }

    fn area(self) -> u64 {
        u64::from(self.w) * u64::from(self.h)
    }

    fn touches(self, other: Rect) -> bool {
        u32::from(self.x) <= other.right()
            && u32::from(other.x) <= self.right()
            && u32::from(self.y) <= other.bottom()
            && u32::from(other.y) <= self.bottom()
    }

    fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Rect::new(x, y, (r - u32::from(x)) as u16, (b - u32::from(y)) as u16)
    }

    fn clip(self, width: u16, height: u16) -> Option<Rect> {
        if self.x >= width || self.y >= height {
            return None;
        }
        let w = self.w.min(width - self.x);
        let h = self.h.min(height - self.y);
        (w > 0 && h > 0).then_some(Rect::new(self.x, self.y, w, h))
    }
}

/// Rectangles that already cover most of their bounding box go as that box: one piece is cheaper
/// to paint.
fn add_dirty(rects: &mut Vec<Rect>, rect: Rect) {
    let mut merged = rect;
    // A grown rectangle can reach ones it missed earlier, so rescan until nothing changes.
    loop {
        let before = rects.len();
        rects.retain(|r| {
            let touches = r.touches(merged);
            if touches {
                merged = merged.union(*r);
            }
            !touches
        });
        if rects.len() == before {
            break;
        }
    }
    rects.push(merged);

    let bounds = rects.iter().copied().reduce(Rect::union).unwrap_or(merged);
    let covered: u64 = rects.iter().map(|r| r.area()).sum();
    if rects.len() > MAX_DIRTY_RECTS || covered * 4 >= bounds.area() * 3 {
        rects.clear();
        rects.push(bounds);
    }
}

pub struct FrameStore {
    inner: Mutex<Inner>,
    notify: Notify,
    produced: AtomicU64,
}

struct Inner {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
    dirty: Vec<Rect>,
    lossy: Vec<Rect>,
    refining: bool,
}

impl Default for FrameStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                width: 0,
                height: 0,
                pixels: Vec::new(),
                dirty: Vec::new(),
                lossy: Vec::new(),
                refining: false,
            }),
            notify: Notify::new(),
            produced: AtomicU64::new(0),
        }
    }

    pub fn resize(&self, width: u16, height: u16) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.width = width;
            inner.height = height;
            inner.pixels = vec![0; usize::from(width) * usize::from(height) * 4];
            inner.dirty.clear();
            inner.lossy.clear();
            inner
                .dirty
                .extend(Rect::new(0, 0, width, height).clip(width, height));
        }
        self.mark_produced();
    }

    pub fn blit(&self, rect: Rect, src: &[u8], src_stride: usize) {
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(rect) = rect.clip(inner.width, inner.height) else {
                return;
            };
            let dst_stride = usize::from(inner.width) * 4;
            let row_len = usize::from(rect.w) * 4;
            for row in 0..usize::from(rect.h) {
                let y = usize::from(rect.y) + row;
                let src_off = y * src_stride + usize::from(rect.x) * 4;
                let dst_off = y * dst_stride + usize::from(rect.x) * 4;
                inner.pixels[dst_off..dst_off + row_len]
                    .copy_from_slice(&src[src_off..src_off + row_len]);
            }
            add_dirty(&mut inner.dirty, rect);
        }
        self.mark_produced();
    }

    pub fn draw<R: IntoIterator<Item = Rect>>(&self, draw: impl FnOnce(&mut [u8], u16, u16) -> R) {
        {
            let mut guard = self.inner.lock().unwrap();
            let inner = &mut *guard;
            let (w, h) = (inner.width, inner.height);
            let mut changed = false;
            for rect in draw(&mut inner.pixels, w, h)
                .into_iter()
                .filter_map(|r| r.clip(w, h))
            {
                add_dirty(&mut inner.dirty, rect);
                changed = true;
            }
            if !changed {
                return;
            }
        }
        self.mark_produced();
    }

    pub fn snapshot(&self) -> (u16, u16, Vec<u8>) {
        let inner = self.inner.lock().unwrap();
        (inner.width, inner.height, inner.pixels.clone())
    }

    pub fn size(&self) -> (u16, u16) {
        let inner = self.inner.lock().unwrap();
        (inner.width, inner.height)
    }

    pub fn produced(&self) -> u64 {
        self.produced.load(Ordering::Relaxed)
    }

    fn mark_produced(&self) {
        self.produced.fetch_add(1, Ordering::Relaxed);
        self.notify.notify_one();
    }

    pub(crate) async fn changed(&self) {
        self.notify.notified().await;
    }

    pub(crate) fn take_message(&self, allow_lossy: bool) -> Option<TakenFrame> {
        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;
        let refining = std::mem::take(&mut inner.refining);
        if inner.dirty.is_empty() {
            return None;
        }
        let stride = usize::from(inner.width) * 4;
        let entries: Vec<_> = inner
            .dirty
            .iter()
            .map(|r| {
                (
                    *r,
                    wire::Encoding::Raw,
                    usize::from(r.w) * usize::from(r.h) * 4,
                )
            })
            .collect();
        let mut message = wire::frame_header(inner.width, inner.height, &entries);
        for rect in &inner.dirty {
            let row_len = usize::from(rect.w) * 4;
            for row in 0..usize::from(rect.h) {
                let off = (usize::from(rect.y) + row) * stride + usize::from(rect.x) * 4;
                message.extend_from_slice(&inner.pixels[off..off + row_len]);
            }
        }
        let surface = Rect::new(0, 0, inner.width, inner.height).area();
        let lossy: Vec<Rect> = if allow_lossy && !refining {
            inner
                .dirty
                .iter()
                .copied()
                .filter(|r| r.area() * LOSSY_SHARE >= surface)
                .collect()
        } else {
            Vec::new()
        };
        for rect in &lossy {
            add_dirty(&mut inner.lossy, *rect);
        }
        inner.dirty.clear();
        Some(TakenFrame { message, lossy })
    }

    pub(crate) fn queue_refine(&self) {
        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;
        inner.refining = true;
        for rect in std::mem::take(&mut inner.lossy) {
            add_dirty(&mut inner.dirty, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty(rects: &[Rect]) -> Vec<Rect> {
        let mut list = Vec::new();
        for r in rects {
            add_dirty(&mut list, *r);
        }
        list
    }

    #[test]
    fn union_covers_both() {
        let r = Rect::new(10, 10, 5, 5).union(Rect::new(0, 20, 2, 2));
        assert_eq!(r, Rect::new(0, 10, 15, 12));
    }

    #[test]
    fn clip_trims_and_rejects() {
        assert_eq!(
            Rect::new(8, 8, 10, 10).clip(10, 10),
            Some(Rect::new(8, 8, 2, 2))
        );
        assert_eq!(Rect::new(10, 0, 1, 1).clip(10, 10), None);
    }

    #[test]
    fn distant_rects_stay_separate() {
        let (a, b) = (Rect::new(0, 0, 10, 10), Rect::new(500, 500, 10, 10));
        assert_eq!(dirty(&[a, b]), [a, b]);
    }

    #[test]
    fn overlapping_rects_merge() {
        let list = dirty(&[Rect::new(0, 0, 10, 10), Rect::new(5, 5, 10, 10)]);
        assert_eq!(list, [Rect::new(0, 0, 15, 15)]);
    }

    #[test]
    fn touching_rects_merge() {
        let list = dirty(&[Rect::new(0, 0, 10, 10), Rect::new(10, 0, 10, 10)]);
        assert_eq!(list, [Rect::new(0, 0, 20, 10)]);
    }

    #[test]
    fn a_grown_rect_absorbs_rects_it_now_reaches() {
        // The new rect touches only the tall one, but their union reaches the first one,
        // which comes earlier in the list.
        let list = dirty(&[
            Rect::new(0, 0, 10, 10),
            Rect::new(20, 0, 10, 100),
            Rect::new(0, 50, 25, 10),
        ]);
        assert_eq!(list, [Rect::new(0, 0, 30, 100)]);
    }

    #[test]
    fn the_list_holds_up_to_the_limit() {
        let rects: Vec<_> = (0..MAX_DIRTY_RECTS as u16)
            .map(|i| Rect::new(i * 10, 0, 1, 1))
            .collect();
        assert_eq!(dirty(&rects), rects);
    }

    #[test]
    fn overflow_falls_back_to_the_bounding_box() {
        let rects: Vec<_> = (0..=MAX_DIRTY_RECTS as u16)
            .map(|i| Rect::new(i * 10, i * 10, 1, 1))
            .collect();
        assert_eq!(dirty(&rects), [Rect::new(0, 0, 161, 161)]);
    }

    #[test]
    fn rects_covering_most_of_their_box_fall_back_to_it() {
        let list = dirty(&[Rect::new(0, 0, 10, 10), Rect::new(11, 0, 10, 10)]);
        assert_eq!(list, [Rect::new(0, 0, 21, 10)]);
    }

    #[test]
    fn updates_merge_until_taken() {
        let store = FrameStore::new();
        store.resize(4, 4);
        let _ = store.take_message(false);
        let src: Vec<u8> = (0..4 * 4 * 4).collect();
        store.blit(Rect::new(0, 0, 1, 1), &src, 16);
        store.blit(Rect::new(3, 3, 1, 1), &src, 16);
        let msg = store.take_message(false).expect("dirty").message;
        let frame = wire::parse_frame(&msg).unwrap();
        assert_eq!((frame.surface_w, frame.surface_h), (4, 4));
        let rects: Vec<_> = frame.rects.iter().map(|r| (r.rect, r.data)).collect();
        assert_eq!(
            rects,
            [
                (Rect::new(0, 0, 1, 1), &src[0..4]),
                (Rect::new(3, 3, 1, 1), &src[60..64])
            ]
        );
        assert!(store.take_message(false).is_none());
    }

    #[test]
    fn draw_marks_every_returned_rect() {
        let store = FrameStore::new();
        store.resize(100, 100);
        let _ = store.take_message(false);
        store.draw(|_, _, _| {
            [
                Rect::new(0, 0, 2, 2),
                Rect::new(50, 50, 2, 2),
                Rect::new(200, 0, 2, 2),
            ]
        });
        let msg = store.take_message(false).expect("dirty").message;
        let rects: Vec<_> = wire::parse_frame(&msg)
            .unwrap()
            .rects
            .iter()
            .map(|r| r.rect)
            .collect();
        assert_eq!(rects, [Rect::new(0, 0, 2, 2), Rect::new(50, 50, 2, 2)]);
    }

    #[test]
    fn resize_replaces_pending_rects_with_the_whole_surface() {
        let store = FrameStore::new();
        store.resize(4, 4);
        store.draw(|_, _, _| Some(Rect::new(0, 0, 1, 1)));
        store.resize(8, 8);
        let msg = store.take_message(false).expect("dirty").message;
        let frame = wire::parse_frame(&msg).unwrap();
        assert_eq!(frame.rects.len(), 1);
        assert_eq!(frame.rects[0].rect, Rect::new(0, 0, 8, 8));
    }

    #[test]
    fn a_snapshot_holds_the_pixels_and_leaves_updates_pending() {
        let store = FrameStore::new();
        store.resize(2, 1);
        let src = [1, 2, 3, 4, 5, 6, 7, 8];
        store.blit(Rect::new(0, 0, 2, 1), &src, 8);
        assert_eq!(store.snapshot(), (2, 1, src.to_vec()));
        assert!(store.take_message(false).is_some());
    }

    fn store_with(rect: Rect) -> FrameStore {
        let store = FrameStore::new();
        store.resize(100, 100);
        let _ = store.take_message(false);
        store.draw(|_, _, _| Some(rect));
        store
    }

    #[test]
    fn only_large_rects_may_go_lossy() {
        let large = Rect::new(0, 0, 50, 50);
        assert_eq!(store_with(large).take_message(true).unwrap().lossy, [large]);
        assert!(
            store_with(Rect::new(0, 0, 49, 50))
                .take_message(true)
                .unwrap()
                .lossy
                .is_empty()
        );
        assert!(
            store_with(large)
                .take_message(false)
                .unwrap()
                .lossy
                .is_empty()
        );
    }

    #[test]
    fn refining_resends_lossy_rects_losslessly_once() {
        let large = Rect::new(0, 0, 50, 50);
        let store = store_with(large);
        let _ = store.take_message(true);

        store.queue_refine();
        let refined = store.take_message(true).expect("lossy area is dirty again");
        assert!(refined.lossy.is_empty());
        let rects = wire::parse_frame(&refined.message).unwrap();
        assert_eq!(rects.rects[0].rect, large);

        // Nothing is left to refine, and later large updates may go lossy again.
        store.queue_refine();
        assert!(store.take_message(true).is_none());
        store.draw(|_, _, _| Some(large));
        assert_eq!(store.take_message(true).unwrap().lossy, [large]);
    }
}
