//! Commit-graph lane assignment.
//!
//! Walks commits in topological order and assigns each a horizontal lane so
//! the frontend can render branch lines without any layout logic. The lane
//! state (`active`) carries across pagination: each entry is the Oid that a
//! lane is "waiting for" further down the walk.

use git2::Oid;

#[derive(Debug, Default, Clone)]
pub struct LaneState {
  active: Vec<Option<Oid>>,
}

pub struct LaneAssignment {
  pub lane: u32,
  /// Lane index for each parent, aligned with the commit's parent order.
  pub parent_lanes: Vec<u32>,
}

impl LaneState {
  pub fn assign(&mut self, oid: Oid, parents: &[Oid]) -> LaneAssignment {
    // Find the lane already expecting this commit, else allocate lowest free.
    let lane = match self.active.iter().position(|slot| *slot == Some(oid)) {
      Some(i) => i,
      None => self.alloc(),
    };
    // Any OTHER lane also expecting this commit (branch point) collapses here.
    for slot in self.active.iter_mut() {
      if *slot == Some(oid) {
        *slot = None;
      }
    }

    let mut parent_lanes = Vec::with_capacity(parents.len());
    for (pi, parent) in parents.iter().enumerate() {
      if pi == 0 {
        // First parent continues in this commit's lane.
        self.active[lane] = Some(*parent);
        parent_lanes.push(lane as u32);
      } else if let Some(existing) = self.active.iter().position(|s| *s == Some(*parent)) {
        // Merge into a lane already waiting for this parent.
        parent_lanes.push(existing as u32);
      } else {
        let l = self.alloc();
        self.active[l] = Some(*parent);
        parent_lanes.push(l as u32);
      }
    }
    if parents.is_empty() {
      self.active[lane] = None;
    }

    // Trim trailing dead lanes so alloc stays compact.
    while matches!(self.active.last(), Some(None)) {
      self.active.pop();
    }

    LaneAssignment { lane: lane as u32, parent_lanes }
  }

  fn alloc(&mut self) -> usize {
    match self.active.iter().position(|s| s.is_none()) {
      Some(i) => i,
      None => {
        self.active.push(None);
        self.active.len() - 1
      }
    }
  }
}

pub fn initials(name: &str) -> String {
  let mut it = name.split_whitespace().filter_map(|w| w.chars().next());
  let first = it.next().unwrap_or('?');
  let second = it.last();
  match second {
    Some(c) => format!("{}{}", first, c).to_uppercase(),
    None => {
      let mut chars = name.chars();
      let a = chars.next().unwrap_or('?');
      match chars.next() {
        Some(b) => format!("{}{}", a, b).to_uppercase(),
        None => a.to_uppercase().to_string(),
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn oid(n: u8) -> Oid {
    let mut bytes = [0u8; 20];
    bytes[0] = n;
    Oid::from_bytes(&bytes).unwrap()
  }

  #[test]
  fn linear_history_stays_in_lane_zero() {
    let mut s = LaneState::default();
    let a = s.assign(oid(1), &[oid(2)]);
    let b = s.assign(oid(2), &[oid(3)]);
    let c = s.assign(oid(3), &[]);
    assert_eq!(a.lane, 0);
    assert_eq!(b.lane, 0);
    assert_eq!(c.lane, 0);
  }

  #[test]
  fn merge_allocates_second_lane_and_branch_rejoins() {
    let mut s = LaneState::default();
    // merge commit M with parents A (mainline) and B (feature)
    let m = s.assign(oid(1), &[oid(2), oid(3)]);
    assert_eq!(m.lane, 0);
    assert_eq!(m.parent_lanes, vec![0, 1]);
    // feature commit B sits in lane 1
    let b = s.assign(oid(3), &[oid(4)]);
    assert_eq!(b.lane, 1);
    // mainline A in lane 0
    let a = s.assign(oid(2), &[oid(4)]);
    assert_eq!(a.lane, 0);
    // fork point C: both lanes converge; takes lane 0, lane 1 freed
    let c = s.assign(oid(4), &[oid(5)]);
    assert_eq!(c.lane, 0);
    let d = s.assign(oid(5), &[]);
    assert_eq!(d.lane, 0);
  }

  #[test]
  fn initials_variants() {
    assert_eq!(initials("Dana K."), "DK");
    assert_eq!(initials("Priya"), "PR");
    assert_eq!(initials("Marco Luis Perez"), "MP");
  }
}
