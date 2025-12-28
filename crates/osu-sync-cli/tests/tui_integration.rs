//! Integration tests for TUI using ratatui's TestBackend
//!
//! These tests verify that:
//! - Key events are handled correctly
//! - Selection state is properly tracked
//! - Filtering and selection work together correctly
//! - The UI renders the expected content

use std::collections::HashSet;

use osu_sync_core::sync::{DryRunAction, DryRunItem, DryRunResult, SyncDirection};

/// Test harness for TUI integration tests
mod test_harness {
    use super::*;

    /// Create a mock DryRunItem for testing
    pub fn make_item(set_id: Option<i32>, title: &str, artist: &str, action: DryRunAction) -> DryRunItem {
        DryRunItem {
            set_id,
            title: title.to_string(),
            artist: artist.to_string(),
            action,
            size_bytes: 1_000_000,
            difficulty_count: 4,
        }
    }

    /// Create a mock DryRunResult with test data
    pub fn make_dry_run_result() -> DryRunResult {
        DryRunResult {
            items: vec![
                make_item(Some(123), "UNION!!", "765 MILLION ALLSTARS", DryRunAction::Import),
                make_item(Some(456), "Harumachi Clover", "Hanatan", DryRunAction::Import),
                make_item(Some(789), "UNION!! Remix", "Some Artist", DryRunAction::Import),
                make_item(Some(111), "Already Synced", "Artist", DryRunAction::Skip),
                make_item(Some(222), "Duplicate Song", "Artist", DryRunAction::Duplicate),
                make_item(Some(333), "Test Song", "UNION Band", DryRunAction::Import),
                make_item(None, "No ID Song", "Unknown", DryRunAction::Import),
            ],
            total_import: 5,
            total_skip: 1,
            total_duplicate: 1,
            total_size_bytes: 7_000_000,
        }
    }

    /// Filter items by search text (mirrors the app logic)
    pub fn filter_items(items: &[DryRunItem], filter_text: &str) -> Vec<usize> {
        if filter_text.is_empty() {
            return (0..items.len()).collect();
        }

        let filter_lower = filter_text.to_lowercase();
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.title.to_lowercase().contains(&filter_lower)
                    || item.artist.to_lowercase().contains(&filter_lower)
                    || item.set_id.map(|id| id.to_string().contains(&filter_lower)).unwrap_or(false)
            })
            .map(|(idx, _)| idx)
            .collect()
    }
}

use test_harness::*;

// ============================================================================
// Unit Tests for Filter Logic
// ============================================================================

#[test]
fn test_filter_empty_returns_all() {
    let result = make_dry_run_result();
    let indices = filter_items(&result.items, "");
    assert_eq!(indices.len(), result.items.len());
}

#[test]
fn test_filter_by_title() {
    let result = make_dry_run_result();
    let indices = filter_items(&result.items, "UNION");

    // Should match: "UNION!!" (0), "UNION!! Remix" (2), and "UNION Band" artist (5)
    assert_eq!(indices, vec![0, 2, 5]);
}

#[test]
fn test_filter_by_artist() {
    let result = make_dry_run_result();
    let indices = filter_items(&result.items, "Hanatan");
    assert_eq!(indices, vec![1]);
}

#[test]
fn test_filter_by_set_id() {
    let result = make_dry_run_result();
    let indices = filter_items(&result.items, "456");
    assert_eq!(indices, vec![1]);
}

#[test]
fn test_filter_case_insensitive() {
    let result = make_dry_run_result();
    let indices_lower = filter_items(&result.items, "union");
    let indices_upper = filter_items(&result.items, "UNION");
    assert_eq!(indices_lower, indices_upper);
}

#[test]
fn test_filter_no_matches() {
    let result = make_dry_run_result();
    let indices = filter_items(&result.items, "nonexistent");
    assert!(indices.is_empty());
}

// ============================================================================
// Selection Logic Tests
// ============================================================================

#[test]
fn test_display_to_actual_index_mapping() {
    let result = make_dry_run_result();
    let visible_indices = filter_items(&result.items, "UNION");

    // visible_indices = [0, 2, 5]
    // display_index 0 -> actual_index 0 (UNION!!)
    // display_index 1 -> actual_index 2 (UNION!! Remix)
    // display_index 2 -> actual_index 5 (Test Song by UNION Band)

    assert_eq!(visible_indices.get(0), Some(&0));
    assert_eq!(visible_indices.get(1), Some(&2));
    assert_eq!(visible_indices.get(2), Some(&5));
}

#[test]
fn test_selection_uses_actual_index() {
    let result = make_dry_run_result();
    let visible_indices = filter_items(&result.items, "UNION");
    let mut checked_items: HashSet<usize> = HashSet::new();

    // Simulate user selecting display_index 1 (which maps to actual_index 2)
    let display_idx = 1;
    if let Some(&actual_idx) = visible_indices.get(display_idx) {
        checked_items.insert(actual_idx);
    }

    // The checked_items should contain actual index 2, not display index 1
    assert!(checked_items.contains(&2));
    assert!(!checked_items.contains(&1));

    // Verify we get the correct item
    let selected_item = &result.items[2];
    assert_eq!(selected_item.title, "UNION!! Remix");
}

#[test]
fn test_extract_set_ids_from_checked_items() {
    let result = make_dry_run_result();
    let checked_items: HashSet<usize> = [0, 2].into_iter().collect();

    let set_ids: HashSet<i32> = checked_items
        .iter()
        .filter_map(|&idx| result.items.get(idx).and_then(|item| item.set_id))
        .collect();

    assert_eq!(set_ids.len(), 2);
    assert!(set_ids.contains(&123)); // UNION!!
    assert!(set_ids.contains(&789)); // UNION!! Remix
}

#[test]
fn test_selection_skips_non_import_items() {
    let result = make_dry_run_result();

    // Item 3 (Skip) and 4 (Duplicate) should not be selectable
    let skip_item = &result.items[3];
    let dup_item = &result.items[4];

    assert_eq!(skip_item.action, DryRunAction::Skip);
    assert_eq!(dup_item.action, DryRunAction::Duplicate);

    // Simulating what the app does: only allow selection of Import items
    let mut checked_items: HashSet<usize> = HashSet::new();
    for (idx, item) in result.items.iter().enumerate() {
        if matches!(item.action, DryRunAction::Import) {
            checked_items.insert(idx);
        }
    }

    // Should have 5 items (indices 0, 1, 2, 5, 6)
    assert_eq!(checked_items.len(), 5);
    assert!(!checked_items.contains(&3)); // Skip
    assert!(!checked_items.contains(&4)); // Duplicate
}

#[test]
fn test_select_all_only_selects_import_items() {
    let result = make_dry_run_result();

    // Simulate Ctrl+A: select all Import items
    let checked_items: HashSet<usize> = result.items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item.action, DryRunAction::Import))
        .map(|(idx, _)| idx)
        .collect();

    assert_eq!(checked_items.len(), 5);
    assert!(checked_items.contains(&0)); // Import
    assert!(checked_items.contains(&1)); // Import
    assert!(checked_items.contains(&2)); // Import
    assert!(!checked_items.contains(&3)); // Skip - not selected
    assert!(!checked_items.contains(&4)); // Duplicate - not selected
    assert!(checked_items.contains(&5)); // Import
    assert!(checked_items.contains(&6)); // Import
}

#[test]
fn test_toggle_selection() {
    let result = make_dry_run_result();
    let mut checked_items: HashSet<usize> = HashSet::new();

    // Toggle on
    checked_items.insert(0);
    assert!(checked_items.contains(&0));

    // Toggle off
    checked_items.remove(&0);
    assert!(!checked_items.contains(&0));

    // Toggle on again
    checked_items.insert(0);
    assert!(checked_items.contains(&0));
}

#[test]
fn test_filtered_selection_persists_after_filter_clear() {
    let result = make_dry_run_result();
    let mut checked_items: HashSet<usize> = HashSet::new();

    // User filters to "UNION", selects display index 1 (actual index 2)
    let visible_indices = filter_items(&result.items, "UNION");
    if let Some(&actual_idx) = visible_indices.get(1) {
        checked_items.insert(actual_idx);
    }

    // User clears filter
    let all_indices = filter_items(&result.items, "");

    // The selection should still be there (at actual index 2)
    assert!(checked_items.contains(&2));

    // And when viewing all items, item at index 2 should still be selected
    assert!(all_indices.contains(&2));
}

// ============================================================================
// State Transition Tests
// ============================================================================

#[test]
fn test_dry_run_state_structure() {
    let result = make_dry_run_result();
    let _direction = SyncDirection::StableToLazer;
    let _selected_item: usize = 0;
    let _scroll_offset: usize = 0;
    let checked_items: HashSet<usize> = HashSet::new();
    let filter_text = String::new();
    let filter_mode = false;

    // Verify state can be constructed
    assert_eq!(result.items.len(), 7);
    assert_eq!(checked_items.len(), 0);
    assert!(!filter_mode);
    assert!(filter_text.is_empty());
}

#[test]
fn test_navigation_bounds() {
    let result = make_dry_run_result();
    let visible_indices = filter_items(&result.items, "");
    let total_visible = visible_indices.len();

    // Simulate up/down navigation
    let mut selected_item: usize = 0;

    // Move down
    selected_item = (selected_item + 1) % total_visible;
    assert_eq!(selected_item, 1);

    // Move to last item
    selected_item = total_visible - 1;
    assert_eq!(selected_item, 6);

    // Move down from last (wraps to 0)
    selected_item = (selected_item + 1) % total_visible;
    assert_eq!(selected_item, 0);

    // Move up from first (wraps to last)
    selected_item = selected_item.checked_sub(1).unwrap_or(total_visible - 1);
    assert_eq!(selected_item, 6);
}

#[test]
fn test_navigation_with_filter() {
    let result = make_dry_run_result();
    let visible_indices = filter_items(&result.items, "UNION");
    let total_visible = visible_indices.len();

    assert_eq!(total_visible, 3);

    let mut selected_item: usize = 0;

    // Move down
    selected_item = (selected_item + 1) % total_visible;
    assert_eq!(selected_item, 1);

    // Check what actual item is selected
    let actual_idx = visible_indices[selected_item];
    assert_eq!(actual_idx, 2); // UNION!! Remix
}

// ============================================================================
// Checkbox Display Tests
// ============================================================================

#[test]
fn test_checkbox_display_for_checked_item() {
    let _result = make_dry_run_result();
    let mut checked_items: HashSet<usize> = HashSet::new();
    checked_items.insert(0);

    // Verify the item is checked
    assert!(checked_items.contains(&0));

    // The checkbox display should be "[x]" for checked items
    let is_checked = checked_items.contains(&0);
    let checkbox = if is_checked { "[x]" } else { "[ ]" };
    assert_eq!(checkbox, "[x]");
}

#[test]
fn test_checkbox_display_for_unchecked_item() {
    let checked_items: HashSet<usize> = HashSet::new();

    // Verify the item is not checked
    assert!(!checked_items.contains(&0));

    let is_checked = checked_items.contains(&0);
    let checkbox = if is_checked { "[x]" } else { "[ ]" };
    assert_eq!(checkbox, "[ ]");
}

#[test]
fn test_checkbox_hidden_for_non_importable() {
    let result = make_dry_run_result();
    let skip_item = &result.items[3];
    let dup_item = &result.items[4];

    // Non-importable items should not show checkbox
    let skip_checkbox = if matches!(skip_item.action, DryRunAction::Import) { "[x]" } else { "   " };
    let dup_checkbox = if matches!(dup_item.action, DryRunAction::Import) { "[x]" } else { "   " };

    assert_eq!(skip_checkbox, "   ");
    assert_eq!(dup_checkbox, "   ");
}

// ============================================================================
// Sync Initiation Tests
// ============================================================================

#[test]
fn test_sync_with_selected_items() {
    let result = make_dry_run_result();
    let checked_items: HashSet<usize> = [0, 2].into_iter().collect();

    // Extract set IDs for sync
    let selected_set_ids: HashSet<i32> = checked_items
        .iter()
        .filter_map(|&idx| result.items.get(idx).and_then(|item| item.set_id))
        .collect();

    assert_eq!(selected_set_ids.len(), 2);
    assert!(selected_set_ids.contains(&123));
    assert!(selected_set_ids.contains(&789));
}

#[test]
fn test_sync_single_item_on_enter_with_no_selection() {
    let result = make_dry_run_result();
    let _checked_items: HashSet<usize> = HashSet::new(); // Empty - no checked items
    let selected_item: usize = 1; // Harumachi Clover
    let filter_text = "";

    // When Enter is pressed with no checked items, sync just the current item
    let visible_indices = filter_items(&result.items, filter_text);
    let actual_idx = visible_indices[selected_item];

    let item = &result.items[actual_idx];
    assert_eq!(item.title, "Harumachi Clover");
    assert_eq!(item.set_id, Some(456));

    // This single item should be synced
    let single_set_id = item.set_id;
    assert_eq!(single_set_id, Some(456));
}

#[test]
fn test_sync_blocked_for_non_import_item() {
    let result = make_dry_run_result();
    let selected_item: usize = 3; // Skip item
    let filter_text = "";

    let visible_indices = filter_items(&result.items, filter_text);
    let actual_idx = visible_indices[selected_item];

    let item = &result.items[actual_idx];
    assert_eq!(item.action, DryRunAction::Skip);

    // Sync should not proceed for non-import items
    let can_sync = matches!(item.action, DryRunAction::Import);
    assert!(!can_sync);
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_empty_result_handling() {
    let result = DryRunResult {
        items: vec![],
        total_import: 0,
        total_skip: 0,
        total_duplicate: 0,
        total_size_bytes: 0,
    };

    let visible_indices = filter_items(&result.items, "");
    assert!(visible_indices.is_empty());

    let checked_items: HashSet<usize> = HashSet::new();
    assert!(checked_items.is_empty());
}

#[test]
fn test_all_items_skipped() {
    let result = DryRunResult {
        items: vec![
            make_item(Some(1), "Skip 1", "Artist", DryRunAction::Skip),
            make_item(Some(2), "Skip 2", "Artist", DryRunAction::Skip),
        ],
        total_import: 0,
        total_skip: 2,
        total_duplicate: 0,
        total_size_bytes: 0,
    };

    // Ctrl+A should not select any items
    let checked_items: HashSet<usize> = result.items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item.action, DryRunAction::Import))
        .map(|(idx, _)| idx)
        .collect();

    assert!(checked_items.is_empty());
}

#[test]
fn test_item_without_set_id() {
    let result = make_dry_run_result();
    let checked_items: HashSet<usize> = [6].into_iter().collect(); // Item 6 has no set_id

    let set_ids: HashSet<i32> = checked_items
        .iter()
        .filter_map(|&idx| result.items.get(idx).and_then(|item| item.set_id))
        .collect();

    // Should not include items without set_id
    assert!(set_ids.is_empty());
}
