//! AudioRepo — Git-style version control for DSP state.
//! Authority: lineos/docs/audio-git-spec-v1_0.md v1.2
//! INV-GIT-1..6

use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DspState {
    pub ducking_depth: f32,
    pub sidechain_hold: usize,
    pub ms_width: f32,
    pub lfe_gain: f32,
}

impl Default for DspState {
    fn default() -> Self {
        Self {
            ducking_depth: 1.0,
            sidechain_hold: 3,
            ms_width: 1.0,
            lfe_gain: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MixCommit {
    pub hash: String,
    pub parent_hash: Option<String>,
    pub timestamp: u64,
    pub message: String,
    pub state: DspState,
}

pub struct AudioRepo {
    pub commits: HashMap<String, MixCommit>,
    pub branches: HashMap<String, String>,
    pub active_branch: String,
    pub head_state: Arc<ArcSwap<DspState>>,
}

impl AudioRepo {
    pub fn new(initial_state: DspState) -> Self {
        let hash = uuid::Uuid::new_v4().to_string();
        let commit = MixCommit {
            hash: hash.clone(),
            parent_hash: None,
            timestamp: 0,
            message: "Initial Mix".to_string(),
            state: initial_state,
        };
        let mut commits = HashMap::new();
        let mut branches = HashMap::new();
        commits.insert(hash.clone(), commit);
        branches.insert("main".to_string(), hash);
        Self {
            commits,
            branches,
            active_branch: "main".to_string(),
            head_state: Arc::new(ArcSwap::new(Arc::new(initial_state))),
        }
    }

    /// Create repo with system flavour branches pre-loaded.
    /// Called at AppState init — replaces new() in production.
    /// INV-FLAVOUR-1: system branches never deleted.
    pub fn new_with_flavours(initial_state: DspState) -> Self {
        let mut repo = Self::new(initial_state);
        for (name, state) in crate::flavours::ALL {
            repo.create_branch(name)
                .unwrap_or(());  // ignore if already exists
            let current = repo.active_branch.clone();
            repo.checkout(name).unwrap_or(());
            repo.commit(*state, &format!("{} preset", name));
            repo.checkout(&current).unwrap_or(());
        }
        // Always return to main
        let _ = repo.checkout("main");
        repo
    }

    pub fn commit(&mut self, new_state: DspState, message: &str) -> String {
        let parent = self.branches.get(&self.active_branch).cloned();
        let hash = uuid::Uuid::new_v4().to_string();
        let commit = MixCommit {
            hash: hash.clone(),
            parent_hash: parent,
            timestamp: 0,
            message: message.to_string(),
            state: new_state,
        };
        self.commits.insert(hash.clone(), commit);
        self.branches
            .insert(self.active_branch.clone(), hash.clone());
        self.head_state.store(Arc::new(new_state));
        hash
    }

    pub fn checkout(&mut self, branch_name: &str) -> Result<(), String> {
        let hash = self
            .branches
            .get(branch_name)
            .ok_or_else(|| format!("Branch '{}' not found", branch_name))?
            .clone();
        let state = self.commits.get(&hash).ok_or("Commit not found")?.state;
        self.active_branch = branch_name.to_string();
        self.head_state.store(Arc::new(state));
        Ok(())
    }

    pub fn revert_head(&mut self) -> Result<(), String> {
        let current_hash = self
            .branches
            .get(&self.active_branch)
            .ok_or("No HEAD")?
            .clone();
        let parent_hash = self
            .commits
            .get(&current_hash)
            .and_then(|c| c.parent_hash.clone())
            .ok_or("No parent commit — already at root")?;
        let parent_state = self
            .commits
            .get(&parent_hash)
            .ok_or("Parent commit not found")?
            .state;
        self.branches
            .insert(self.active_branch.clone(), parent_hash);
        self.head_state.store(Arc::new(parent_state));
        Ok(())
    }

    pub fn create_branch(&mut self, name: &str) -> Result<(), String> {
        if self.branches.contains_key(name) {
            return Err(format!("Branch '{}' already exists", name));
        }
        let current = self
            .branches
            .get(&self.active_branch)
            .ok_or("No HEAD")?
            .clone();
        self.branches.insert(name.to_string(), current);
        Ok(())
    }

    pub fn head_state(&self) -> Arc<DspState> {
        self.head_state.load_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_initialization() {
        let s = DspState::default();
        let repo = AudioRepo::new(s);
        assert!(repo.branches.contains_key("main"));
        assert_eq!(repo.active_branch, "main");
        assert_eq!(repo.commits.len(), 1);
        assert_eq!(*repo.head_state(), s);
    }

    #[test]
    fn test_commit_chaining() {
        let mut repo = AudioRepo::new(DspState::default());
        let s1 = DspState {
            ducking_depth: 1.2,
            ..Default::default()
        };
        let s2 = DspState {
            ducking_depth: 1.5,
            ..Default::default()
        };
        let h1 = repo.commit(s1, "More punch");
        let h2 = repo.commit(s2, "Club mix");
        assert_eq!(repo.commits[&h2].parent_hash, Some(h1.clone()));
        assert_eq!(repo.commits[&h1].parent_hash.is_some(), true);
    }

    #[test]
    fn test_zero_latency_checkout() {
        let mut repo = AudioRepo::new(DspState::default());
        repo.create_branch("club_mix").unwrap();
        repo.checkout("club_mix").unwrap();
        repo.commit(
            DspState {
                ducking_depth: 1.5,
                ..Default::default()
            },
            "Club settings",
        );
        assert!((repo.head_state().ducking_depth - 1.5).abs() < 0.001);
        repo.checkout("main").unwrap();
        assert!((repo.head_state().ducking_depth - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_revert_undo() {
        let s0 = DspState::default();
        let s1 = DspState {
            ducking_depth: 1.5,
            ..Default::default()
        };
        let mut repo = AudioRepo::new(s0);
        let h1 = repo.commit(s1, "Aggressive");
        repo.revert_head().unwrap();
        assert!((repo.head_state().ducking_depth - 1.0).abs() < 0.001);
        // child commit still exists (INV-GIT-3)
        assert!(repo.commits.contains_key(&h1));
    }

    #[test]
    fn test_create_branch() {
        let mut repo = AudioRepo::new(DspState::default());
        repo.create_branch("radio_edit").unwrap();
        assert!(repo.branches.contains_key("radio_edit"));
        // duplicate branch fails
        assert!(repo.create_branch("radio_edit").is_err());
    }

    #[test]
    fn test_modifier_math() {
        let ai_base = 0.707_f32;
        let club = 1.5_f32;
        let radio = 0.8_f32;
        let final_club = (ai_base / club).clamp(0.1, 1.0);
        let final_radio = (ai_base / radio).clamp(0.1, 1.0);
        assert!((final_club - 0.471).abs() < 0.001);
        assert!((final_radio - 0.884).abs() < 0.001);
    }
}
