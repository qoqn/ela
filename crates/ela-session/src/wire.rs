use crate::Rect;

pub const MSG_TERMINAL: u8 = 1;
pub const MSG_FRAME: u8 = 2;
pub const FRAME_HEADER_LEN: usize = 7;
pub const FRAME_RECT_LEN: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Encoding {
    Raw = 0,
    Jpeg = 1,
}

#[derive(Debug)]
pub struct Frame<'a> {
    pub surface_w: u16,
    pub surface_h: u16,
    pub rects: Vec<FrameRect<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FrameRect<'a> {
    pub rect: Rect,
    pub encoding: Encoding,
    pub data: &'a [u8],
}

pub fn frame_header(surface_w: u16, surface_h: u16, rects: &[(Rect, Encoding, usize)]) -> Vec<u8> {
    let data_len: usize = rects.iter().map(|(_, _, len)| len).sum();
    let mut msg = Vec::with_capacity(FRAME_HEADER_LEN + rects.len() * FRAME_RECT_LEN + data_len);
    msg.push(MSG_FRAME);
    for v in [surface_w, surface_h, rects.len() as u16] {
        msg.extend_from_slice(&v.to_le_bytes());
    }
    for (rect, encoding, len) in rects {
        for v in [rect.x, rect.y, rect.w, rect.h] {
            msg.extend_from_slice(&v.to_le_bytes());
        }
        msg.push(*encoding as u8);
        msg.extend_from_slice(&(*len as u32).to_le_bytes());
    }
    msg
}

pub fn parse_frame(msg: &[u8]) -> Option<Frame<'_>> {
    if msg.len() < FRAME_HEADER_LEN || msg[0] != MSG_FRAME {
        return None;
    }
    let u16_at = |off: usize| u16::from_le_bytes([msg[off], msg[off + 1]]);
    let count = usize::from(u16_at(5));
    let mut data = msg.get(FRAME_HEADER_LEN + count * FRAME_RECT_LEN..)?;
    let mut rects = Vec::with_capacity(count);
    for i in 0..count {
        let off = FRAME_HEADER_LEN + i * FRAME_RECT_LEN;
        let rect = Rect::new(
            u16_at(off),
            u16_at(off + 2),
            u16_at(off + 4),
            u16_at(off + 6),
        );
        let encoding = match msg[off + 8] {
            0 => Encoding::Raw,
            1 => Encoding::Jpeg,
            _ => return None,
        };
        let len = u32::from_le_bytes([msg[off + 9], msg[off + 10], msg[off + 11], msg[off + 12]])
            as usize;
        if encoding == Encoding::Raw && len != usize::from(rect.w) * usize::from(rect.h) * 4 {
            return None;
        }
        let (bytes, rest) = data.split_at_checked(len)?;
        rects.push(FrameRect {
            rect,
            encoding,
            data: bytes,
        });
        data = rest;
    }
    data.is_empty().then_some(Frame {
        surface_w: u16_at(1),
        surface_h: u16_at(3),
        rects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Vec<u8> {
        let entries = [
            (Rect::new(1, 2, 1, 1), Encoding::Raw, 4),
            (Rect::new(3, 4, 200, 100), Encoding::Jpeg, 3),
        ];
        let mut msg = frame_header(10, 20, &entries);
        msg.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7]);
        msg
    }

    #[test]
    fn frame_round_trip() {
        let msg = message();
        let frame = parse_frame(&msg).unwrap();
        assert_eq!((frame.surface_w, frame.surface_h), (10, 20));
        assert_eq!(
            frame.rects,
            [
                FrameRect {
                    rect: Rect::new(1, 2, 1, 1),
                    encoding: Encoding::Raw,
                    data: &[1, 2, 3, 4]
                },
                FrameRect {
                    rect: Rect::new(3, 4, 200, 100),
                    encoding: Encoding::Jpeg,
                    data: &[5, 6, 7]
                },
            ]
        );
    }

    #[test]
    fn wrong_lengths_and_types_are_rejected() {
        let msg = message();
        assert!(parse_frame(&msg[..msg.len() - 1]).is_none());
        assert!(parse_frame(&msg[..FRAME_HEADER_LEN + 3]).is_none());
        assert!(parse_frame(&[msg.as_slice(), &[0]].concat()).is_none());
        assert!(parse_frame(&[MSG_TERMINAL, 0, 0, 0, 0, 0, 0]).is_none());

        let wrong_raw_len = frame_header(10, 20, &[(Rect::new(0, 0, 2, 2), Encoding::Raw, 4)]);
        assert!(parse_frame(&[wrong_raw_len.as_slice(), &[0; 4]].concat()).is_none());

        let mut unknown_encoding = message();
        unknown_encoding[FRAME_HEADER_LEN + 8] = 9;
        assert!(parse_frame(&unknown_encoding).is_none());
    }
}
