//! Inline vector with heap spillover for small-vec optimization.
//!
//! Provides `InlineVec<T, N>` which stores up to N elements inline,
//! spilling to heap when capacity is exceeded. Uses safe Rust only.
//!
//! Only compiled when no tinyvec/smallvec features are enabled.

#![cfg(not(any(
    feature = "_tinyvec-64-bytes",
    feature = "_tinyvec-128-bytes",
    feature = "_tinyvec-256-bytes",
    feature = "_tinyvec-512-bytes",
    feature = "_smallvec-128-bytes",
    feature = "_smallvec-256-bytes"
)))]

use alloc::vec::Vec;

/// A vector that stores up to N elements inline, spilling to heap when exceeded.
///
/// This provides small-vec optimization without unsafe code by using `Option<T>`
/// for inline slots. For pointer types like `Option<&T>`, there's zero overhead
/// due to niche optimization.
///
/// # Type Parameters
/// - `T`: Element type (must be `Copy` for efficient operations)
/// - `N`: Number of inline slots (compile-time constant)
pub struct InlineVec<T: Copy, const N: usize> {
    /// Number of elements stored (inline or heap).
    len: u8,
    /// Inline storage slots. Uses Option<T> for safe initialization.
    inline: [Option<T>; N],
    /// Heap storage for overflow. Empty until first spillover.
    /// Uses Vec directly (not Box<Vec>) to avoid double indirection.
    heap: Vec<T>,
}

impl<T: Copy, const N: usize> InlineVec<T, N> {
    /// Create a new empty InlineVec.
    #[inline]
    pub const fn new() -> Self {
        Self {
            len: 0,
            inline: [None; N],
            heap: Vec::new(),
        }
    }

    /// Returns the number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns true if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Try to push an element. Returns false on allocation failure.
    #[inline]
    pub fn try_push(&mut self, value: T) -> bool {
        let idx = self.len as usize;

        if idx < N {
            // Store inline
            self.inline[idx] = Some(value);
            self.len += 1;
            true
        } else {
            // Spill to heap
            if self.heap.try_reserve(1).is_err() {
                return false;
            }
            self.heap.push(value);
            self.len += 1;
            true
        }
    }

    /// Push an element (panics on allocation failure).
    #[allow(dead_code)] // Part of complete API
    #[inline]
    pub fn push(&mut self, value: T) {
        if !self.try_push(value) {
            panic!("InlineVec: allocation failed");
        }
    }

    /// Pop the last element.
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let idx = (self.len - 1) as usize;
        self.len -= 1;

        if idx < N {
            // Pop from inline
            self.inline[idx].take()
        } else {
            // Pop from heap
            self.heap.pop()
        }
    }

    /// Get element at index.
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

    /// Remove and return the first element, shifting others left.
    /// Returns None if empty.
    #[allow(dead_code)] // Part of complete API
    #[inline]
    pub fn remove_first(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        // Get the first element
        let first = if N > 0 {
            self.inline[0].take()
        } else {
            Some(self.heap.remove(0))
        };

        // Shift inline elements left
        for i in 0..N.saturating_sub(1) {
            self.inline[i] = self.inline[i + 1].take();
        }

        // If we have heap elements, move one to the last inline slot
        if !self.heap.is_empty() && N > 0 {
            self.inline[N - 1] = Some(self.heap.remove(0));
        }

        self.len -= 1;
        first
    }

    /// Insert an element at the beginning, shifting others right.
    /// Returns false on allocation failure.
    #[inline]
    pub fn insert_first(&mut self, value: T) -> bool {
        // If we need to spill, ensure heap capacity first
        if self.len as usize >= N && self.heap.try_reserve(1).is_err() {
            return false;
        }

        // If we have N inline elements, move the last one to heap
        if self.len as usize >= N && N > 0 {
            if let Some(last_inline) = self.inline[N - 1].take() {
                self.heap.insert(0, last_inline);
            }
        }

        // Shift inline elements right
        for i in (1..N).rev() {
            self.inline[i] = self.inline[i - 1].take();
        }

        // Insert at front
        if N > 0 {
            self.inline[0] = Some(value);
        } else {
            self.heap.insert(0, value);
        }

        self.len += 1;
        true
    }

    /// Remove element at index, shifting remaining elements left.
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

        // Shift elements left
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

    /// Insert element at index, shifting elements right.
    #[allow(dead_code)] // Part of complete API
    #[inline]
    pub fn insert(&mut self, index: usize, value: T) {
        if index > self.len as usize {
            panic!("index out of bounds");
        }

        // Ensure capacity for spillover
        if self.len as usize >= N {
            self.heap.reserve(1);
        }

        // Move last inline to heap if needed
        if self.len as usize >= N && N > 0 {
            if let Some(last) = self.inline[N - 1].take() {
                self.heap.insert(0, last);
            }
        }

        // Shift inline elements right from index
        for i in (index + 1..N).rev() {
            self.inline[i] = self.inline[i - 1].take();
        }

        // Insert
        if index < N {
            self.inline[index] = Some(value);
        } else {
            self.heap.insert(index - N, value);
        }

        self.len += 1;
    }

    /// Iterate over elements.
    #[inline]
    pub fn iter(&self) -> InlineVecIter<'_, T, N> {
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

/// Iterator over InlineVec elements.
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_new_is_empty() {
        let v: InlineVec<i32, 4> = InlineVec::new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn test_push_within_inline() {
        let mut v: InlineVec<i32, 4> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        v.push(4);
        assert_eq!(v.len(), 4);
        assert!(v.heap.is_empty()); // Still inline
    }

    #[test]
    fn test_push_spills_to_heap() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3); // Spills
        assert_eq!(v.len(), 3);
        assert!(!v.heap.is_empty());
    }

    #[test]
    fn test_pop() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        assert_eq!(v.pop(), Some(3)); // From heap
        assert_eq!(v.pop(), Some(2)); // From inline
        assert_eq!(v.pop(), Some(1)); // From inline
        assert_eq!(v.pop(), None);
    }

    #[test]
    fn test_get() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.push(10);
        v.push(20);
        v.push(30);
        assert_eq!(v.get(0), Some(10));
        assert_eq!(v.get(1), Some(20));
        assert_eq!(v.get(2), Some(30));
        assert_eq!(v.get(3), None);
    }

    #[test]
    fn test_iter() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_iter_rev() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        let collected: Vec<_> = v.iter().rev().collect();
        assert_eq!(collected, vec![3, 2, 1]);
    }

    #[test]
    fn test_remove_first() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        assert_eq!(v.remove_first(), Some(1));
        assert_eq!(v.len(), 2);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![2, 3]);
    }

    #[test]
    fn test_insert_first() {
        let mut v: InlineVec<i32, 2> = InlineVec::new();
        v.push(2);
        v.push(3);
        assert!(v.insert_first(1));
        assert_eq!(v.len(), 3);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_remove() {
        let mut v: InlineVec<i32, 4> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        assert_eq!(v.remove(1), 2);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 3]);
    }

    #[test]
    fn test_insert() {
        let mut v: InlineVec<i32, 4> = InlineVec::new();
        v.push(1);
        v.push(3);
        v.insert(1, 2);
        let collected: Vec<_> = v.iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_exact_size_iter() {
        let mut v: InlineVec<i32, 4> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
        let iter = v.iter();
        assert_eq!(iter.len(), 3);
    }
}
