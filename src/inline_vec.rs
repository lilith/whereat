//! Unified inline vector abstraction with multiple backend support.
//!
//! `InlineVec<T, N>` provides a consistent API regardless of which storage
//! backend is selected via feature flags:
//! - Default: Custom inline+heap implementation (4 inline slots)
//! - `_tinyvec-*`: Uses `tinyvec::TinyVec`
//! - `_smallvec-*`: Uses `smallvec::SmallVec`

// ============================================================================
// TinyVec backend
// ============================================================================

#[cfg(any(
    feature = "_tinyvec-64-bytes",
    feature = "_tinyvec-128-bytes",
    feature = "_tinyvec-256-bytes",
    feature = "_tinyvec-512-bytes",
))]
mod backend {
    use tinyvec::TinyVec;

    /// InlineVec backed by TinyVec.
    #[derive(Debug)]
    pub struct InlineVec<T: Default, const N: usize>(TinyVec<[T; N]>);

    impl<T: Default + Copy, const N: usize> InlineVec<T, N> {
        #[inline]
        pub fn new() -> Self {
            Self(TinyVec::new())
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        #[inline]
        pub fn try_push(&mut self, value: T) -> bool {
            self.0.push(value);
            true
        }

        #[inline]
        pub fn pop(&mut self) -> Option<T> {
            self.0.pop()
        }

        #[allow(dead_code)] // Part of complete API
        #[inline]
        pub fn get(&self, index: usize) -> Option<T> {
            self.0.get(index).copied()
        }

        #[inline]
        pub fn remove(&mut self, index: usize) -> T {
            self.0.remove(index)
        }

        #[allow(dead_code)] // Part of complete API
        #[inline]
        pub fn insert(&mut self, index: usize, value: T) {
            self.0.insert(index, value);
        }

        #[inline]
        pub fn insert_first(&mut self, value: T) -> bool {
            self.0.insert(0, value);
            true
        }

        /// Iterate over elements (yields T directly via copied).
        #[inline]
        pub fn iter(&self) -> impl DoubleEndedIterator<Item = T> + ExactSizeIterator + '_ {
            self.0.iter().copied()
        }
    }

    impl<T: Default + Copy, const N: usize> Default for InlineVec<T, N> {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ============================================================================
// SmallVec backend
// ============================================================================

#[cfg(all(
    any(feature = "_smallvec-128-bytes", feature = "_smallvec-256-bytes"),
    not(any(
        feature = "_tinyvec-64-bytes",
        feature = "_tinyvec-128-bytes",
        feature = "_tinyvec-256-bytes",
        feature = "_tinyvec-512-bytes",
    ))
))]
mod backend {
    use smallvec::SmallVec;

    /// InlineVec backed by SmallVec.
    #[derive(Debug)]
    pub struct InlineVec<T, const N: usize>(SmallVec<[T; N]>);

    impl<T: Copy, const N: usize> InlineVec<T, N> {
        #[inline]
        pub fn new() -> Self {
            Self(SmallVec::new())
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        #[inline]
        pub fn try_push(&mut self, value: T) -> bool {
            self.0.push(value);
            true
        }

        #[inline]
        pub fn pop(&mut self) -> Option<T> {
            self.0.pop()
        }

        #[allow(dead_code)] // Part of complete API
        #[inline]
        pub fn get(&self, index: usize) -> Option<T> {
            self.0.get(index).copied()
        }

        #[inline]
        pub fn remove(&mut self, index: usize) -> T {
            self.0.remove(index)
        }

        #[allow(dead_code)] // Part of complete API
        #[inline]
        pub fn insert(&mut self, index: usize, value: T) {
            self.0.insert(index, value);
        }

        #[inline]
        pub fn insert_first(&mut self, value: T) -> bool {
            self.0.insert(0, value);
            true
        }

        /// Iterate over elements (yields T directly via copied).
        #[inline]
        pub fn iter(&self) -> impl DoubleEndedIterator<Item = T> + ExactSizeIterator + '_ {
            self.0.iter().copied()
        }
    }

    impl<T: Copy, const N: usize> Default for InlineVec<T, N> {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ============================================================================
// Custom inline+heap backend (default)
// ============================================================================

#[cfg(not(any(
    feature = "_tinyvec-64-bytes",
    feature = "_tinyvec-128-bytes",
    feature = "_tinyvec-256-bytes",
    feature = "_tinyvec-512-bytes",
    feature = "_smallvec-128-bytes",
    feature = "_smallvec-256-bytes"
)))]
mod backend {
    use alloc::vec::Vec;

    /// InlineVec with custom inline+heap storage.
    pub struct InlineVec<T: Copy, const N: usize> {
        /// Number of elements stored (inline or heap).
        len: u8,
        /// Inline storage slots. Uses Option<T> for safe initialization.
        inline: [Option<T>; N],
        /// Heap storage for overflow.
        heap: Vec<T>,
    }

    impl<T: Copy, const N: usize> InlineVec<T, N> {
        #[inline]
        pub const fn new() -> Self {
            Self {
                len: 0,
                inline: [None; N],
                heap: Vec::new(),
            }
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.len as usize
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.len == 0
        }

        #[inline]
        pub fn try_push(&mut self, value: T) -> bool {
            let idx = self.len as usize;
            if idx < N {
                self.inline[idx] = Some(value);
                self.len += 1;
                true
            } else {
                if self.heap.try_reserve(1).is_err() {
                    return false;
                }
                self.heap.push(value);
                self.len += 1;
                true
            }
        }

        #[inline]
        pub fn pop(&mut self) -> Option<T> {
            if self.len == 0 {
                return None;
            }
            let idx = (self.len - 1) as usize;
            self.len -= 1;
            if idx < N {
                self.inline[idx].take()
            } else {
                self.heap.pop()
            }
        }

        #[inline]
        pub fn get(&self, index: usize) -> Option<T> {
            if index >= self.len as usize {
                return None;
            }
            if index < N {
                self.inline[index]
            } else {
                self.heap.get(index - N).copied()
            }
        }

        #[inline]
        pub fn remove(&mut self, index: usize) -> T {
            if index >= self.len as usize {
                panic!("index out of bounds");
            }
            let result = if index < N {
                self.inline[index].take().unwrap()
            } else {
                self.heap.remove(index - N)
            };
            // Shift inline elements left
            for i in index..N.saturating_sub(1) {
                self.inline[i] = self.inline[i + 1].take();
            }
            // Move heap element to inline if needed
            if !self.heap.is_empty() && index < N {
                self.inline[N - 1] = Some(self.heap.remove(0));
            }
            self.len -= 1;
            result
        }

        #[allow(dead_code)] // Part of complete API
        #[inline]
        pub fn insert(&mut self, index: usize, value: T) {
            if index > self.len as usize {
                panic!("index out of bounds");
            }
            if self.len as usize >= N {
                self.heap.reserve(1);
            }
            if self.len as usize >= N && N > 0 {
                if let Some(last) = self.inline[N - 1].take() {
                    self.heap.insert(0, last);
                }
            }
            for i in (index + 1..N).rev() {
                self.inline[i] = self.inline[i - 1].take();
            }
            if index < N {
                self.inline[index] = Some(value);
            } else {
                self.heap.insert(index - N, value);
            }
            self.len += 1;
        }

        #[inline]
        pub fn insert_first(&mut self, value: T) -> bool {
            if self.len as usize >= N && self.heap.try_reserve(1).is_err() {
                return false;
            }
            if self.len as usize >= N && N > 0 {
                if let Some(last_inline) = self.inline[N - 1].take() {
                    self.heap.insert(0, last_inline);
                }
            }
            for i in (1..N).rev() {
                self.inline[i] = self.inline[i - 1].take();
            }
            if N > 0 {
                self.inline[0] = Some(value);
            } else {
                self.heap.insert(0, value);
            }
            self.len += 1;
            true
        }

        /// Iterate over elements (yields T directly).
        #[inline]
        pub fn iter(&self) -> impl DoubleEndedIterator<Item = T> + ExactSizeIterator + '_ {
            InlineVecIter {
                vec: self,
                front: 0,
                back: self.len as usize,
            }
        }
    }

    impl<T: Copy, const N: usize> Default for InlineVec<T, N> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T: Copy + core::fmt::Debug, const N: usize> core::fmt::Debug for InlineVec<T, N> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_list().entries(self.iter()).finish()
        }
    }

    /// Iterator for custom InlineVec backend.
    pub struct InlineVecIter<'a, T: Copy, const N: usize> {
        vec: &'a InlineVec<T, N>,
        front: usize,
        back: usize,
    }

    impl<T: Copy, const N: usize> Iterator for InlineVecIter<'_, T, N> {
        type Item = T;

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            if self.front >= self.back {
                return None;
            }
            let item = self.vec.get(self.front);
            self.front += 1;
            item
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            let len = self.back.saturating_sub(self.front);
            (len, Some(len))
        }
    }

    impl<T: Copy, const N: usize> DoubleEndedIterator for InlineVecIter<'_, T, N> {
        #[inline]
        fn next_back(&mut self) -> Option<Self::Item> {
            if self.front >= self.back {
                return None;
            }
            self.back -= 1;
            self.vec.get(self.back)
        }
    }

    impl<T: Copy, const N: usize> ExactSizeIterator for InlineVecIter<'_, T, N> {}
}

// Re-export from the active backend
pub use backend::InlineVec;

// ============================================================================
// Tests (only for default backend)
// ============================================================================

#[cfg(all(
    test,
    not(any(
        feature = "_tinyvec-64-bytes",
        feature = "_tinyvec-128-bytes",
        feature = "_tinyvec-256-bytes",
        feature = "_tinyvec-512-bytes",
        feature = "_smallvec-128-bytes",
        feature = "_smallvec-256-bytes"
    ))
))]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn test_new_is_empty() {
        let v: InlineVec<i32, 4> = InlineVec::new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn test_push_pop() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        assert!(v.try_push(1));
        assert!(v.try_push(2));
        assert!(v.try_push(3)); // Spills to heap
        assert_eq!(v.len(), 3);
        assert_eq!(v.pop(), Some(3));
        assert_eq!(v.pop(), Some(2));
        assert_eq!(v.pop(), Some(1));
        assert_eq!(v.pop(), None);
    }

    #[test]
    fn test_get() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.try_push(10);
        v.try_push(20);
        v.try_push(30);
        assert_eq!(v.get(0), Some(10));
        assert_eq!(v.get(1), Some(20));
        assert_eq!(v.get(2), Some(30));
        assert_eq!(v.get(3), None);
    }

    #[test]
    fn test_iter() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.try_push(1);
        v.try_push(2);
        v.try_push(3);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_iter_rev() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.try_push(1);
        v.try_push(2);
        v.try_push(3);
        let collected: Vec<_> = v.iter().rev().collect();
        assert_eq!(collected, vec![3, 2, 1]);
    }

    #[test]
    fn test_remove() {
        let mut v: InlineVec<i32, 4> = InlineVec::new();
        v.try_push(1);
        v.try_push(2);
        v.try_push(3);
        assert_eq!(v.remove(1), 2);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 3]);
    }

    #[test]
    fn test_insert() {
        let mut v: InlineVec<i32, 4> = InlineVec::new();
        v.try_push(1);
        v.try_push(3);
        v.insert(1, 2);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_insert_first() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.try_push(2);
        v.try_push(3);
        assert!(v.insert_first(1));
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_exact_size_iter() {
        let mut v: InlineVec<i32, 4> = InlineVec::new();
        v.try_push(1);
        v.try_push(2);
        v.try_push(3);
        let iter = v.iter();
        assert_eq!(iter.len(), 3);
    }
}
