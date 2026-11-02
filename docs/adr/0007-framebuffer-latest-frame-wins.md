# Framebuffer updates: latest frame wins

For RDP, VNC and other Framebuffer Sessions, the backend merges dirty rectangles into a single pending update, and the UI takes the newest one when it has painted the last. It paints on arrival, not in an animation frame: an update there waited a frame for its paint, and the update after it waited another for the credit that paint returned. What's drawn still reaches the screen with the next rendering update. Intermediate frames are dropped rather than queued. A stale remote screen has no value, and queueing frames over IPC under load (video, a slow WebKitGTK paint) grows latency and memory without bound. Terminal output is the opposite case: every byte matters, so it is never dropped and goes through a bounded channel that slows the reader instead.

The pending update holds up to 16 rectangles, so small changes far apart don't become one almost full-screen update. Rectangles that overlap or touch are merged. If the list overflows, or the rectangles cover at least 75% of their bounding box, the update falls back to that box.

A rectangle covering at least a quarter of the screen goes as JPEG, unless the Connection's `image_quality` is `lossless`. Once no lossy update has gone out for 200 ms, the areas sent lossy are sent again losslessly, so a still screen ends up exact.
