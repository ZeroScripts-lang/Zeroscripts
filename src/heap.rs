use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeapRef {
    pub(crate) index: usize,
    pub(crate) generation: u64,
}

impl HeapRef {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl fmt::Display for HeapRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<heap ref {}:{}>", self.index, self.generation)
    }
}

#[derive(Debug, Clone)]
pub struct EnvBinding {
    pub name: String,
    pub value: HeapRef,
    pub is_const: bool,
}

#[derive(Debug, Clone)]
pub enum HeapObject {
    Str(String),
    Number(f64),
    Boolean(bool),
    Null,
    List {
        items: Vec<HeapRef>,
        is_const: bool,
    },
    Object {
        props: Vec<(String, HeapRef)>,
        is_const: bool,
    },
    Function {
        index: usize,
        name: String,
        params: Vec<String>,
        entry_point: usize,
    },
    Environment(Vec<EnvBinding>),
    Closure {
        function_index: usize,
        env: HeapRef,
        param_defaults: Vec<Option<usize>>,
    },
    /// Task handle (Async.md §101): stores the VM task index. Task state
    /// lives in the VM's task table, not on the heap.
    Task(usize),
}

impl HeapObject {
    pub fn list(items: Vec<HeapRef>) -> Self {
        HeapObject::List {
            items,
            is_const: false,
        }
    }

    pub fn object(props: Vec<(String, HeapRef)>) -> Self {
        HeapObject::Object {
            props,
            is_const: false,
        }
    }

    pub fn is_const_collection(&self) -> bool {
        match self {
            HeapObject::List { is_const, .. } | HeapObject::Object { is_const, .. } => *is_const,
            _ => false,
        }
    }

    pub fn mark_const_collection(&mut self) {
        match self {
            HeapObject::List { is_const, .. } | HeapObject::Object { is_const, .. } => {
                *is_const = true
            }
            _ => {}
        }
    }
}

impl HeapObject {
    pub fn kind(&self) -> HeapObjectKind {
        match self {
            HeapObject::Str(_) => HeapObjectKind::String,
            HeapObject::Number(_) => HeapObjectKind::Number,
            HeapObject::Boolean(_) => HeapObjectKind::Boolean,
            HeapObject::Null => HeapObjectKind::Null,
            HeapObject::List { .. } => HeapObjectKind::List,
            HeapObject::Object { .. } => HeapObjectKind::Object,
            HeapObject::Function { .. } => HeapObjectKind::Function,
            HeapObject::Environment(_) => HeapObjectKind::Environment,
            HeapObject::Closure { .. } => HeapObjectKind::Closure,
            HeapObject::Task(_) => HeapObjectKind::Task,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapObjectKind {
    String,
    Number,
    Boolean,
    Null,
    List,
    Object,
    Function,
    Environment,
    Closure,
    Task,
}

struct HeapSlot {
    object: HeapObject,
    generation: u64,
    alive: bool,
    marked: bool,
}

pub struct Heap {
    slots: Vec<HeapSlot>,
    free_list: Vec<usize>,
    generation: u64,
}

impl Heap {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_list: Vec::new(),
            generation: 1,
        }
    }

    pub fn allocate(&mut self, object: HeapObject) -> HeapRef {
        let current_gen = self.generation;
        if let Some(index) = self.free_list.pop() {
            self.slots[index] = HeapSlot {
                object,
                generation: current_gen,
                alive: true,
                marked: false,
            };
            HeapRef { index, generation: current_gen }
        } else {
            let index = self.slots.len();
            self.slots.push(HeapSlot {
                object,
                generation: current_gen,
                alive: true,
                marked: false,
            });
            HeapRef { index, generation: current_gen }
        }
    }

    pub fn get(&self, reference: &HeapRef) -> Result<&HeapObject, HeapError> {
        if reference.index >= self.slots.len() {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef index {} out of bounds (len {})",
                reference.index,
                self.slots.len()
            )));
        }

        let slot = &self.slots[reference.index];

        if !slot.alive || slot.generation != reference.generation {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef {}:{} is no longer valid",
                reference.index, reference.generation
            )));
        }

        Ok(&slot.object)
    }

    pub fn get_mut(&mut self, reference: &HeapRef) -> Result<&mut HeapObject, HeapError> {
        if reference.index >= self.slots.len() {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef index {} out of bounds (len {})",
                reference.index,
                self.slots.len()
            )));
        }

        let slot = &mut self.slots[reference.index];

        if !slot.alive || slot.generation != reference.generation {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef {}:{} is no longer valid",
                reference.index, reference.generation
            )));
        }

        Ok(&mut slot.object)
    }

    pub fn is_valid(&self, reference: &HeapRef) -> bool {
        if let Some(slot) = self.slots.get(reference.index) {
            slot.alive && slot.generation == reference.generation
        } else {
            false
        }
    }

    pub fn contains(&self, reference: &HeapRef) -> bool {
        self.is_valid(reference)
    }

    pub fn deallocate(&mut self, reference: &HeapRef) -> Result<(), HeapError> {
        if reference.index >= self.slots.len() {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef index {} out of bounds",
                reference.index
            )));
        }

        let slot = &mut self.slots[reference.index];

        if !slot.alive || slot.generation != reference.generation {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef {}:{} is no longer valid",
                reference.index, reference.generation
            )));
        }

        slot.alive = false;
        slot.generation = self.generation;
        self.generation += 1;
        self.free_list.push(reference.index);
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.slots.iter().filter(|s| s.alive).count()
    }

    pub fn total_slots(&self) -> usize {
        self.slots.len()
    }

    pub fn mark(&mut self, reference: &HeapRef) -> Result<(), HeapError> {
        if reference.index >= self.slots.len() {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef index {} out of bounds",
                reference.index
            )));
        }
        let slot = &mut self.slots[reference.index];
        if !slot.alive || slot.generation != reference.generation {
            return Err(HeapError::InvalidReference(format!(
                "HeapRef {}:{} is no longer valid",
                reference.index, reference.generation
            )));
        }
        slot.marked = true;
        Ok(())
    }

    pub fn is_marked(&self, reference: &HeapRef) -> bool {
        if let Some(slot) = self.slots.get(reference.index) {
            slot.alive && slot.generation == reference.generation && slot.marked
        } else {
            false
        }
    }

    pub fn clear_marks(&mut self) {
        for slot in &mut self.slots {
            slot.marked = false;
        }
    }

    pub fn child_refs(&self, reference: &HeapRef) -> Result<Vec<HeapRef>, HeapError> {
        let obj = self.get(reference)?;
        match obj {
            HeapObject::List { items, .. } => Ok(items.clone()),
            HeapObject::Object { props, .. } => Ok(props.iter().map(|(_, r)| r.clone()).collect()),
            HeapObject::Environment(bindings) => {
                Ok(bindings.iter().map(|b| b.value.clone()).collect())
            }
            HeapObject::Closure { env, .. } => Ok(vec![env.clone()]),
            HeapObject::Str(_) | HeapObject::Function { .. } | HeapObject::Number(_)
            | HeapObject::Boolean(_) | HeapObject::Null | HeapObject::Task(_) => Ok(vec![]),
        }
    }

    pub fn sweep(&mut self) -> SweepResult {
        let mut result = SweepResult {
            freed: 0,
            survived: 0,
            freed_indices: Vec::new(),
        };

        for i in 0..self.slots.len() {
            if self.slots[i].alive && !self.slots[i].marked {
                self.slots[i].alive = false;
                self.slots[i].generation = self.generation;
                self.generation += 1;
                self.free_list.push(i);
                result.freed += 1;
                result.freed_indices.push(i);
            } else if self.slots[i].alive && self.slots[i].marked {
                result.survived += 1;
            }
        }

        result
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn get_env_binding(&self, env_ref: &HeapRef, name: &str) -> Result<Option<HeapRef>, HeapError> {
        match self.get(env_ref)? {
            HeapObject::Environment(bindings) => {
                for b in bindings {
                    if b.name == name {
                        return Ok(Some(b.value.clone()));
                    }
                }
                Ok(None)
            }
            _ => Err(HeapError::InvalidReference("Not an environment".to_string())),
        }
    }

    pub fn get_env_binding_const(&self, env_ref: &HeapRef, name: &str) -> Result<Option<bool>, HeapError> {
        match self.get(env_ref)? {
            HeapObject::Environment(bindings) => {
                for b in bindings {
                    if b.name == name {
                        return Ok(Some(b.is_const));
                    }
                }
                Ok(None)
            }
            _ => Err(HeapError::InvalidReference("Not an environment".to_string())),
        }
    }

    pub fn set_env_binding(&mut self, env_ref: &HeapRef, name: String, value: HeapRef, is_const: bool) -> Result<(), HeapError> {
        match self.get_mut(env_ref)? {
            HeapObject::Environment(bindings) => {
                for b in bindings.iter_mut() {
                    if b.name == name {
                        b.value = value;
                        // Once const, always const
                        if is_const {
                            b.is_const = true;
                        }
                        return Ok(());
                    }
                }
                bindings.push(EnvBinding {
                    name,
                    value,
                    is_const,
                });
                Ok(())
            }
            _ => Err(HeapError::InvalidReference("Not an environment".to_string())),
        }
    }
}

pub struct SweepResult {
    pub freed: usize,
    pub survived: usize,
    pub freed_indices: Vec<usize>,
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeapError {
    InvalidReference(String),
    AllocationFailed(String),
}

impl fmt::Display for HeapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeapError::InvalidReference(msg) => write!(f, "Invalid heap reference: {}", msg),
            HeapError::AllocationFailed(msg) => write!(f, "Heap allocation failed: {}", msg),
        }
    }
}

impl std::error::Error for HeapError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_allocate_and_get() {
        let mut heap = Heap::new();
        let r = heap.allocate(HeapObject::Str("hello".to_string()));
        let obj = heap.get(&r).unwrap();
        match obj {
            HeapObject::Str(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_heap_get_mut() {
        let mut heap = Heap::new();
        let elem = heap.allocate(HeapObject::Str("item".to_string()));
        let r = heap.allocate(HeapObject::list(vec![]));
        {
            let obj = heap.get_mut(&r).unwrap();
            if let HeapObject::List { items, .. } = obj {
                items.push(elem);
            }
        }
        let obj = heap.get(&r).unwrap();
        match obj {
            HeapObject::List { items, .. } => assert_eq!(items.len(), 1),
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_heap_is_valid() {
        let mut heap = Heap::new();
        let r = heap.allocate(HeapObject::Str("test".to_string()));
        assert!(heap.is_valid(&r));

        heap.deallocate(&r).unwrap();
        assert!(!heap.is_valid(&r));
    }

    #[test]
    fn test_heap_invalid_reference() {
        let heap = Heap::new();
        let r = HeapRef {
            index: 0,
            generation: 1,
        };
        assert!(heap.get(&r).is_err());
    }

    #[test]
    fn test_heap_generation_invalidation() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("first".to_string()));
        let r1_copy = HeapRef {
            index: r1.index,
            generation: r1.generation,
        };

        heap.deallocate(&r1).unwrap();
        let _r2 = heap.allocate(HeapObject::Str("second".to_string()));

        assert!(!heap.is_valid(&r1_copy));
        assert!(heap.get(&r1_copy).is_err());
    }

    #[test]
    fn test_heap_free_list_reuse() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("a".to_string()));
        let r2 = heap.allocate(HeapObject::Str("b".to_string()));
        assert_eq!(r1.index, 0);
        assert_eq!(r2.index, 1);

        heap.deallocate(&r1).unwrap();
        let r3 = heap.allocate(HeapObject::Str("c".to_string()));
        assert_eq!(r3.index, 0);
        assert!(heap.is_valid(&r3));
    }

    #[test]
    fn test_heap_count() {
        let mut heap = Heap::new();
        assert_eq!(heap.count(), 0);

        let r1 = heap.allocate(HeapObject::Str("a".to_string()));
        let _r2 = heap.allocate(HeapObject::list(vec![]));
        assert_eq!(heap.count(), 2);

        heap.deallocate(&r1).unwrap();
        assert_eq!(heap.count(), 1);
    }

    #[test]
    fn test_heap_object_kind() {
        let s = HeapObject::Str("x".to_string());
        assert_eq!(s.kind(), HeapObjectKind::String);

        let l = HeapObject::list(vec![]);
        assert_eq!(l.kind(), HeapObjectKind::List);

        let o = HeapObject::object(vec![]);
        assert_eq!(o.kind(), HeapObjectKind::Object);

        let f = HeapObject::Function {
            index: 0,
            name: "test".to_string(),
            params: vec![],
            entry_point: 0,
        };
        assert_eq!(f.kind(), HeapObjectKind::Function);

        let env = HeapObject::Environment(vec![]);
        assert_eq!(env.kind(), HeapObjectKind::Environment);

        let c = HeapObject::Closure {
            function_index: 0,
            env: HeapRef { index: 0, generation: 0 },
            param_defaults: vec![],
        };
        assert_eq!(c.kind(), HeapObjectKind::Closure);
    }

    #[test]
    fn test_multiple_allocations() {
        let mut heap = Heap::new();
        let refs: Vec<HeapRef> = (0..100)
            .map(|i| heap.allocate(HeapObject::Str(format!("str_{}", i))))
            .collect();

        assert_eq!(heap.count(), 100);

        for (i, r) in refs.iter().enumerate() {
            let obj = heap.get(r).unwrap();
            match obj {
                HeapObject::Str(s) => assert_eq!(s, &format!("str_{}", i)),
                _ => panic!("Expected string"),
            }
        }
    }
}
