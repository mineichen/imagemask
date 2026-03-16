# imask

Represents images (e.g. Annotation-Masks) as iterators of ranges

# Error Philosophy

When a Iterator is used, the values are expected to successfuly cast to the value of the accumulator. We expect `SortedRanges::<u16, u16>::iter::<u32>()` not to overflow -> This is checked for debug builds, but not if `cfg!(not(debug_assertions))`, as it causes significant slowdown otherwise. You cannot rely on ranges not to be empty in unsafe code, as overflows are not checked. This library continues processing if this ever happens, but adds some lightweight checks in release-mode (e.g. check, if accumulator is > biggest single element).
