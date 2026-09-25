use crate::heap::{Heap, HeapRef, SweepResult};

#[derive(Debug, Clone, Default)]
pub struct GcStats {
    pub collections: usize,
    pub objects_freed: usize,
    pub objects_survived: usize,
    pub allocations_since_gc: usize,
}

pub struct Gc {
    threshold: usize,
    stats: GcStats,
}

impl Gc {
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            stats: GcStats::default(),
        }
    }

    pub fn stats(&self) -> &GcStats {
        &self.stats
    }

    pub fn stats_mut(&mut self) -> &mut GcStats {
        &mut self.stats
    }

    pub fn should_collect(&self) -> bool {
        self.stats.allocations_since_gc >= self.threshold
    }

    pub fn on_allocation(&mut self) {
        self.stats.allocations_since_gc += 1;
    }

    pub fn collect(&mut self, heap: &mut Heap, roots: &[HeapRef]) -> SweepResult {
        heap.clear_marks();

        for root in roots {
            let _ = heap.mark(root);
        }

        let mut stack: Vec<HeapRef> = roots.to_vec();
        let mut visited = std::collections::HashSet::new();

        while let Some(current) = stack.pop() {
            if !visited.insert((current.index(), current.generation())) {
                continue;
            }

            let _ = heap.mark(&current);

            if let Ok(children) = heap.child_refs(&current) {
                for child in children {
                    if !visited.contains(&(child.index(), child.generation())) {
                        stack.push(child);
                    }
                }
            }
        }

        let result = heap.sweep();

        self.stats.collections += 1;
        self.stats.objects_freed += result.freed;
        self.stats.objects_survived += result.survived;
        self.stats.allocations_since_gc = 0;

        result
    }
}

impl Default for Gc {
    fn default() -> Self {
        Self::new(128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::HeapObject;

    #[test]
    fn test_gc_collect_unreachable_string() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let r1 = heap.allocate(HeapObject::Str("keep".to_string()));
        let r2 = heap.allocate(HeapObject::Str("drop".to_string()));

        assert_eq!(heap.count(), 2);

        let roots = vec![r1.clone()];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 1);
        assert_eq!(result.survived, 1);
        assert_eq!(heap.count(), 1);
        assert!(!heap.is_valid(&r2));
    }

    #[test]
    fn test_gc_collect_unreachable_list() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let elem = heap.allocate(HeapObject::Str("elem".to_string()));
        let list = heap.allocate(HeapObject::list(vec![elem.clone()]));
        let orphan = heap.allocate(HeapObject::Str("orphan".to_string()));

        let roots = vec![list.clone()];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 1);
        assert!(heap.is_valid(&list));
        assert!(heap.is_valid(&elem));
        assert!(!heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_collect_unreachable_object() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let val = heap.allocate(HeapObject::Str("val".to_string()));
        let obj = heap.allocate(HeapObject::object(vec![("key".to_string(), val.clone())]));
        let orphan = heap.allocate(HeapObject::Str("orphan".to_string()));

        let roots = vec![obj.clone()];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 1);
        assert!(heap.is_valid(&obj));
        assert!(heap.is_valid(&val));
        assert!(!heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_collect_unreachable_cycle() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let a = heap.allocate(HeapObject::Str("a".to_string()));
        let b = heap.allocate(HeapObject::Str("b".to_string()));

        if let HeapObject::Str(_) = heap.get(&a).unwrap().clone() {
            *heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        }
        if let HeapObject::Str(_) = heap.get(&b).unwrap().clone() {
            *heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);
        }

        let roots: Vec<HeapRef> = vec![];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 2);
        assert!(!heap.is_valid(&a));
        assert!(!heap.is_valid(&b));
    }

    #[test]
    fn test_gc_collect_reachable_cycle() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let a = heap.allocate(HeapObject::Str("a".to_string()));
        let b = heap.allocate(HeapObject::Str("b".to_string()));

        *heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        let roots = vec![a.clone()];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 0);
        assert!(heap.is_valid(&a));
        assert!(heap.is_valid(&b));
    }

    #[test]
    fn test_gc_mixed_reachable_unreachable() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let r1 = heap.allocate(HeapObject::Str("r1".to_string()));
        let child = heap.allocate(HeapObject::Str("child".to_string()));
        *heap.get_mut(&r1).unwrap() = HeapObject::list(vec![child.clone()]);

        let orphan1 = heap.allocate(HeapObject::Str("orphan1".to_string()));
        let orphan2 = heap.allocate(HeapObject::Str("orphan2".to_string()));

        let roots = vec![r1.clone()];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 2);
        assert!(heap.is_valid(&r1));
        assert!(heap.is_valid(&child));
        assert!(!heap.is_valid(&orphan1));
        assert!(!heap.is_valid(&orphan2));
    }

    #[test]
    fn test_gc_multiple_roots() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let root1 = heap.allocate(HeapObject::Str("root1".to_string()));
        let root2 = heap.allocate(HeapObject::Str("root2".to_string()));
        let orphan = heap.allocate(HeapObject::Str("orphan".to_string()));

        let roots = vec![root1.clone(), root2.clone()];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 1);
        assert!(heap.is_valid(&root1));
        assert!(heap.is_valid(&root2));
        assert!(!heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_should_collect() {
        let mut gc = Gc::new(3);
        assert!(!gc.should_collect());

        gc.on_allocation();
        assert!(!gc.should_collect());

        gc.on_allocation();
        assert!(!gc.should_collect());

        gc.on_allocation();
        assert!(gc.should_collect());
    }

    #[test]
    fn test_gc_stats() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let _r1 = heap.allocate(HeapObject::Str("a".to_string()));
        let _r2 = heap.allocate(HeapObject::Str("b".to_string()));

        let roots: Vec<HeapRef> = vec![];
        gc.collect(&mut heap, &roots);

        assert_eq!(gc.stats().collections, 1);
        assert_eq!(gc.stats().objects_freed, 2);
        assert_eq!(gc.stats().objects_survived, 0);
        assert_eq!(gc.stats().allocations_since_gc, 0);
    }

    #[test]
    fn test_gc_generation_invalidation_after_sweep() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let r1 = heap.allocate(HeapObject::Str("keep".to_string()));
        let r1_copy = HeapRef {
            index: r1.index(),
            generation: r1.generation(),
        };

        let _orphan = heap.allocate(HeapObject::Str("orphan".to_string()));

        let roots = vec![r1.clone()];
        gc.collect(&mut heap, &roots);

        assert!(heap.is_valid(&r1));
        assert!(heap.is_valid(&r1_copy));
    }

    #[test]
    fn test_gc_stale_ref_after_reallocation() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let r1 = heap.allocate(HeapObject::Str("temp".to_string()));
        let roots: Vec<HeapRef> = vec![];
        gc.collect(&mut heap, &roots);

        assert!(!heap.is_valid(&r1));

        let r2 = heap.allocate(HeapObject::Str("new".to_string()));
        assert_eq!(r2.index(), r1.index());
        assert!(heap.is_valid(&r2));
    }

    #[test]
    fn test_gc_deep_graph() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let mut prev = heap.allocate(HeapObject::Str("node0".to_string()));
        let root = prev.clone();

        for i in 1..20 {
            let node = heap.allocate(HeapObject::Str(format!("node{}", i)));
            *heap.get_mut(&prev).unwrap() = HeapObject::list(vec![node.clone()]);
            prev = node;
        }

        let orphan = heap.allocate(HeapObject::Str("orphan".to_string()));

        let roots = vec![root];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 1);
        assert!(!heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_diamond_graph() {
        let mut heap = Heap::new();
        let mut gc = Gc::new(0);

        let shared = heap.allocate(HeapObject::Str("shared".to_string()));
        let left = heap.allocate(HeapObject::list(vec![shared.clone()]));
        let right = heap.allocate(HeapObject::list(vec![shared.clone()]));
        let root = heap.allocate(HeapObject::list(vec![left.clone(), right.clone()]));

        let orphan = heap.allocate(HeapObject::Str("orphan".to_string()));

        let roots = vec![root.clone()];
        let result = gc.collect(&mut heap, &roots);

        assert_eq!(result.freed, 1);
        assert!(heap.is_valid(&root));
        assert!(heap.is_valid(&left));
        assert!(heap.is_valid(&right));
        assert!(heap.is_valid(&shared));
        assert!(!heap.is_valid(&orphan));
    }
}
