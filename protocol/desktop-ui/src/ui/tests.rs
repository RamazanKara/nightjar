//! The launcher's tests: what each page and dialog draws, its focus and its commands.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::*;
use crate::config::{HomeSort, HostRecord};
use crate::host_page::tests::{info as host_answer, FakeProbe};
use crate::host_page::CheckState;
use crate::pads_view::PadsView;

mod fixtures;
mod harness;
use fixtures::*;
use harness::*;

#[path = "settings_tests.rs"]
mod settings_tests;

mod queue_and_navigation_tests;
mod settings_keys_and_fit_tests;
mod layout_and_dialog_tests;
mod host_actions_and_details_tests;
mod host_checks_and_pairing_tests;
mod chrome_and_home_tests;
mod game_page_and_neural_tests;
mod first_run_and_focus_tests;
mod cards_and_hover_tests;
mod pc_capsule_and_focus_tests;
mod polish_and_override_tests;
mod library_and_demo_tests;
mod responsive_and_a11y_tests;
mod pad_links_and_pin_tests;
mod stage_shots;
mod first_run_scan_tests;
mod pairing_panel_tests;
mod host_details_limiter_tests;
