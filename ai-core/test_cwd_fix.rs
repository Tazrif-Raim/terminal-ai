use std::path::Path;
use crate::agent::loop_::cwd_override_allowed;
use crate::agent::types::AgentState;

#[test]
fn test_cwd_override_allowed_with_windows_paths() {
    // Create a mock state with a project root
    let mut state = AgentState {
        project_root: std::env::current_dir().expect("Failed to get current dir"),
        ..Default::default()
    };
    
    // Test that the project root itself is allowed
    assert!(cwd_override_allowed(&state.project_root, &state));
    
    // Test that a subdirectory is allowed
    let subdir = state.project_root.join("test_folder");
    assert!(cwd_override_allowed(&subdir, &state));
    
    // Test that a parent directory is NOT allowed (unless project root has no parent)
    if let Some(parent) = state.project_root.parent() {
        assert!(!cwd_override_allowed(parent, &state));
    }
    
    println!("All tests passed!");
}

fn main() {
    test_cwd_override_allowed_with_windows_paths();
}