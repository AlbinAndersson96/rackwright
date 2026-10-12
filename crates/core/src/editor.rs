use std::mem;

use crate::edit::{CatalogPlan, Plan};
use crate::model::{Catalog, Document};

/// One undo step: the document and the catalogue together.
#[derive(Debug, Clone)]
struct Snapshot {
    document: Document,
    catalog: Catalog,
}

/// The current document and catalogue plus whole-state undo/redo snapshots (session only).
#[derive(Debug, Clone)]
pub struct Editor {
    current: Snapshot,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Editor {
    pub fn new(document: Document, catalog: Catalog) -> Editor {
        Editor {
            current: Snapshot { document, catalog },
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn document(&self) -> &Document {
        &self.current.document
    }

    pub fn catalog(&self) -> &Catalog {
        &self.current.catalog
    }

    pub fn commit(&mut self, plan: Plan) {
        let next = Snapshot {
            document: plan.document,
            catalog: self.current.catalog.clone(),
        };
        self.push(next);
    }

    pub fn commit_catalog(&mut self, plan: CatalogPlan) {
        self.push(Snapshot {
            document: plan.document,
            catalog: plan.catalog,
        });
    }

    fn push(&mut self, next: Snapshot) {
        let previous = mem::replace(&mut self.current, next);
        self.undo.push(previous);
        self.redo.clear();
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(previous) => {
                let current = mem::replace(&mut self.current, previous);
                self.redo.push(current);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                let current = mem::replace(&mut self.current, next);
                self.undo.push(current);
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn replace_clearing_history(&mut self, document: Document, catalog: Catalog) {
        self.current = Snapshot { document, catalog };
        self.undo.clear();
        self.redo.clear();
    }
}
