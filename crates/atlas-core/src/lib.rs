//! Shortwave Atlas core: pure logic with no IO (geo, schedules, station lookup, rig types,
//! and the per-OS rules, which take an `Os` argument so every variant is unit-tested).

pub mod api;
pub mod audio;
pub mod calendar;
pub mod country;
pub mod eibi;
pub mod geo;
pub mod platform;
pub mod rig;
pub mod schedule;
pub mod setup;
pub mod stations;
pub mod text;
pub mod tools;
pub mod update;
