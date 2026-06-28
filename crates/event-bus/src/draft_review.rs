use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DraftStatus { Draft, Review, Approved, Rejected, Merged }

#[derive(Debug)]
pub struct Draft {
    pub id: u16,
    pub agent: String,
    pub summary: String,
    pub data: Vec<u8>,
    pub status: DraftStatus,
    pub review_notes: String,
}

pub struct DraftReview {
    drafts: Vec<Draft>,
    next_id: u16,
}

impl DraftReview {
    pub fn new() -> Self { DraftReview { drafts: Vec::new(), next_id: 1 } }

    pub fn create(&mut self, agent: &str, summary: &str, data: &[u8]) -> u16 {
        let id = self.next_id; self.next_id += 1;
        self.drafts.push(Draft {
            id, agent: String::from(agent), summary: String::from(summary),
            data: data.to_vec(), status: DraftStatus::Draft, review_notes: String::new(),
        });
        id
    }

    pub fn submit(&mut self, id: u16) -> bool {
        if let Some(d) = self.drafts.iter_mut().find(|d| d.id == id) {
            if d.status == DraftStatus::Draft { d.status = DraftStatus::Review; true }
            else { false }
        } else { false }
    }

    pub fn approve(&mut self, id: u16, notes: &str) -> bool {
        if let Some(d) = self.drafts.iter_mut().find(|d| d.id == id) {
            if d.status == DraftStatus::Review {
                d.status = DraftStatus::Approved; d.review_notes = String::from(notes); true
            } else { false }
        } else { false }
    }

    pub fn reject(&mut self, id: u16, notes: &str) -> bool {
        if let Some(d) = self.drafts.iter_mut().find(|d| d.id == id) {
            if d.status == DraftStatus::Review {
                d.status = DraftStatus::Rejected; d.review_notes = String::from(notes); true
            } else { false }
        } else { false }
    }

    pub fn merge(&mut self, id: u16) -> Option<Vec<u8>> {
        if let Some(d) = self.drafts.iter_mut().find(|d| d.id == id) {
            if d.status == DraftStatus::Approved {
                d.status = DraftStatus::Merged;
                Some(d.data.clone())
            } else { None }
        } else { None }
    }

    pub fn pending(&self) -> Vec<&Draft> {
        self.drafts.iter().filter(|d| d.status == DraftStatus::Review).collect()
    }
}
