use std::fmt;
use itertools::Itertools;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Entity {
    name: String,
}

pub struct EntityTree {
    root: Entity,
    next: Vec<EntityTree>,
}

pub struct KarlLabel {
    entity: Entity,
    descendants: Vec<EntityTree>,
    ancestors: Vec<Entity>,
}

impl Entity {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl EntityTree {
    pub fn new(root: Entity, next: Vec<EntityTree>) -> Self {
        EntityTree { root, next }
    }

    pub fn merge(t1: &EntityTree, t2: &EntityTree) -> Option<Self> {
        if t1.root != t2.root {
            return None;
        }
        let new_next: Vec<_> = t1.next.iter()
            .cartesian_product(&t2.next)
            .filter_map(|(t1, t2)| EntityTree::merge(&t1, &t2))
            .collect();
        Some(EntityTree {
            root: t1.root.clone(),
            next: new_next,
        })
    }
}

impl KarlLabel {
    pub fn new(tree: EntityTree) -> Self {
        KarlLabel {
            entity: tree.root,
            descendants: tree.next,
            ancestors: vec![],
        }
    }

    /// Entrust the next entity to handle the data going forwards.
    pub fn next(mut self, next_entity: Entity) -> Result<Self, Self> {
        let mut next_tree_i = None;
        for i in 0..self.descendants.len() {
            if self.descendants.get(i).unwrap().root == next_entity {
                next_tree_i = Some(i);
                break;
            }
        }
        if let Some(i) = next_tree_i {
            let next_tree = self.descendants.remove(i);
            let mut next_label = KarlLabel::new(next_tree);
            next_label.ancestors = self.ancestors;
            next_label.ancestors.push(self.entity);
            return Ok(next_label);
        } else {
            // next entity is not a descendant
            Err(self)
        }
    }

    /// Relabel the entity based on reading data from a stateful edge.
    /// More permissive upstream but less permissive downstream.
    pub fn relabel(self, label: KarlLabel) -> Result<Self, Self> {
        if self.entity == label.entity {
            // Ignore the ancestors of the secondary label
            // Just take the intersection of the descendants
            let t1 = EntityTree::new(self.entity, self.descendants);
            let t2 = EntityTree::new(label.entity, label.descendants);
            let new = EntityTree::merge(&t1, &t2).unwrap();
            Ok(KarlLabel {
                entity: new.root,
                descendants: new.next,
                ancestors: self.ancestors,
            })
        } else {
            // merging invalid entities
            Err(self)
        }
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn source(&self) -> Option<&Entity> {
        self.ancestors.get(0)
    }

    pub fn direct_ancestor(&self) -> Option<&Entity> {
        self.ancestors.get(self.ancestors.len() - 1)
    }

    pub fn descendants(&self) -> Vec<&Entity> {
        self.descendants.iter().map(|tree| &tree.root).collect()
    }
}
