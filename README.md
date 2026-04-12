# imask

Works on top of range-set-blaze to represent image masks as iterator of ranges(e.g. Annotation-Masks). It adds 2-dimensional iterator operators (e.g. dillute, erode) which are orders of magnitude smaller than a bitmap represetation. If range-set-blaze becomes more stable, it will be added as a required dependency. Until then, multiple versions can can be supported via feature-flags. This project aims to upstream general changes to range-set-blaze if they fit.

Collections and wire-formats usually store a ROI (region of interest, which specifies top-left offset (x, y), width and height) of the image mask and store the ranges in the inner coordinate system for maximum storage efficiency. You can ask for both global ranges (IntoIterator) or iter_roi, to get ranges in the ROI coordinate system.
The Iterator-Combinators don't consider the offset and may inherit the ImageWidth from the parent Iterator via `ImageDimension`-trait if it's needed.

# Error Philosophy

When a Iterator is used, the values are expected to successfuly cast to the value of the accumulator. We expect `SortedRanges::<u16, u16>::iter::<u32>()` not to overflow -> This is checked for debug builds, but not if `cfg!(not(debug_assertions))`, as it causes significant slowdown otherwise. You can therefore not rely on ranges not to be empty in unsafe code. This library continues processing if this ever happens, but might add some lightweight assertions in release-mode (e.g. check, if accumulator is > biggest single element).
When comeing from the unchecked places, error-detection is usually provided by returning a Result or having a method `into_result` for cases where error detection can only happen if the `Iterator` was consumed. Iterators stop after a error occurs. If into_result is not called, it causes the iterator to panic if debug_assertions are enabled.

`core::ops::RangeInclusive` and `core::ops::Range` are both expected to have `start < end` in most scenarios, except when loading them from a unchecked iterator.
