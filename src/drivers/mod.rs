//! Every supported device. To add hardware from another maker, write a driver that implements
//! `device::Driver` (see `ciy_x78` for a keyboard and `attack_shark_r5` for a mouse) and add it
//! to the list below.

mod attack_shark_r5;
mod ciy_x78;

use std::sync::Arc;

use crate::device::Driver;

pub fn all() -> Vec<Arc<dyn Driver>> {
    vec![
        Arc::new(ciy_x78::CiyX78::new()),
        Arc::new(attack_shark_r5::AttackSharkR5::new()),
    ]
}
