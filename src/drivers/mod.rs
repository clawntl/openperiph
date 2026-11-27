//! Every supported device. To add hardware from another maker, write a driver that implements
//! `device::Driver` (see `ciy_x78` for a keyboard and `attack_shark_r5` for a mouse) and add it
//! to the list below. QMK keyboards with VIA need no driver of their own: `qmk` covers them all,
//! and keyboards with a bundled VIA definition are listed even while unplugged.

mod attack_shark_r5;
mod ciy_x78;
pub mod qmk;

use std::sync::Arc;

use crate::device::Driver;

pub fn all() -> Vec<Arc<dyn Driver>> {
    let mut v: Vec<Arc<dyn Driver>> = vec![Arc::new(ciy_x78::CiyX78::new()), Arc::new(attack_shark_r5::AttackSharkR5::new())];
    for d in crate::qmk::definition::BUNDLED.iter().filter_map(|t| crate::qmk::definition::Definition::parse(t).ok()) {
        let (vid, pid) = (d.vendor_id, d.product_id);
        // an imported definition for the same keyboard wins
        let d = crate::qmk::definition::all_definitions().into_iter().find(|x| (x.vendor_id, x.product_id) == (vid, pid)).unwrap_or(d);
        let maker = crate::qmk::firmware::source_for(vid, pid).map(|s| s.maker).unwrap_or("QMK");
        v.push(Arc::new(qmk::QmkDriver::new(vid, pid, String::new(), String::new(), maker.into(), Some(d))));
    }
    v
}
