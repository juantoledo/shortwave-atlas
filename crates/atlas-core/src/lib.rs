//! Shortwave Atlas core: pure logic with no IO (geo, schedules, station lookup, rig types,
//! and the per-OS rules, which take an `Os` argument so every variant is unit-tested).

pub mod api;
pub mod audio;
pub mod bands;
pub mod calendar;
pub mod collect;
pub mod country;
pub mod eibi;
pub mod favorites;
pub mod geo;
pub mod hunt;
pub mod lens;
pub mod listening;
pub mod net;
pub mod net_favorites;
pub mod net_listening;
pub mod platform;
pub mod release_notes;
pub mod rig;
pub mod scan;
pub mod schedule;
pub mod setup;
pub mod source;
pub mod stations;
pub mod stats;
pub mod text;
pub mod tools;
pub mod update;
pub mod user_stations;
