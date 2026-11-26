//! Unit tests for controller, touchpad and pen queueing in [`InputQueue`].

use super::*;
use crate::input::packets::{capability, magic, motion_type, touchpad, TouchpadContact};

/// A sent packet: a controller identity (0x55000080), read with the shared decoder the
/// host uses for it (the host core these tests pin predates the packet), or any other
/// input as the host core decodes it.
#[derive(Debug)]
enum Sent {
    Identity(rubylight_protocol::controller_identity::ControllerIdentity),
    Host(Input),
}

fn classify(data: &[u8]) -> Sent {
    use rubylight_protocol::controller_identity::{ControllerIdentity, CONTROLLER_IDENTITY_MAGIC};
    if data.len() >= 8 && data[4..8] == CONTROLLER_IDENTITY_MAGIC.to_le_bytes() {
        Sent::Identity(ControllerIdentity::decode(data).unwrap())
    } else {
        Sent::Host(decode(data).unwrap())
    }
}

/// GFE takes four controllers, Sunshine and its forks 16: an app announces no more than these
/// ([`InputQueue::max_controllers`]) so it can tell the player why a controller does nothing.
#[test]
fn the_controller_slots_follow_the_host() {
    assert_eq!(queue(&gfe()).max_controllers(), 4);
    assert_eq!(queue(&sunshine()).max_controllers(), 16);
}

#[test]
fn touchpad_contacts_go_only_to_a_host_that_takes_them() {
    let contact = TouchpadContact { event: touch_event::MOVE, pointer_id: 1, x: 0.5, y: 0.5, pressure: 0.0 };
    let mut q = queue(&sunshine());
    assert_eq!(q.send_touchpad_frame(&[contact], 0, 0, 0), Err(InputError::Unsupported));
    assert_eq!(q.send_touchpad(contact, 0, 0, 0), Err(InputError::Unsupported));

    let caps = HostCapabilities {
        feature_flags: feature::TOUCHPAD_EVENTS | feature::TOUCHPAD_FRAME_EVENTS,
        ..sunshine()
    };
    let mut q = queue(&caps);
    assert_eq!(q.send_touchpad_frame(&[contact; 6], 0, 0, 0), Err(InputError::Invalid));
    // Moves drop for newer ones; the click and its release, or a new contact, never.
    q.send_touchpad_frame(&[contact; 2], 0, 105, 70).unwrap();
    q.send_touchpad_frame(&[contact], touchpad::BUTTON_PRIMARY, 105, 70).unwrap();
    q.send_touchpad_frame(&[contact], touchpad::BUTTON_PRIMARY, 105, 70).unwrap();
    q.send_touchpad(contact, 0, 105, 70).unwrap();
    q.send_touchpad(TouchpadContact { event: touch_event::DOWN, ..contact }, 0, 105, 70).unwrap();
    let mut out = Vec::new();
    q.poll(Instant::now(), false, &mut out);
    let sent: Vec<(u32, Delivery)> = out
        .iter()
        .map(|p| (u32::from_le_bytes(p.data[4..8].try_into().unwrap()), p.delivery))
        .collect();
    assert_eq!(
        sent,
        [
            (magic::TOUCHPAD_FRAME, Delivery::Unreliable),
            (magic::TOUCHPAD_FRAME, Delivery::Reliable),
            (magic::TOUCHPAD_FRAME, Delivery::Unreliable),
            (magic::TOUCHPAD, Delivery::Reliable),
            (magic::TOUCHPAD, Delivery::Reliable),
        ]
    );
    assert!(out.iter().all(|p| p.channel == crate::control::wire::channel::TOUCH));
}

#[test]
fn controllers_batch_until_buttons_change() {
    let mut q = queue(&sunshine());
    let t0 = Instant::now();
    q.send_controller(1, 3, 0, 0, 0, (100, 0), (0, 0)).unwrap();
    q.send_controller(1, 3, 0, 0, 0, (200, 0), (0, 0)).unwrap();
    q.send_controller(0, 3, button::A, 0, 0, (0, 0), (0, 0))
        .unwrap();
    q.send_controller(1, 3, 0, 9, 0, (300, 0), (0, 0)).unwrap();
    q.send_controller(1, 3, button::B, 0, 0, (400, 0), (0, 0))
        .unwrap();
    q.send_controller(1, 3, button::B, 0, 0, (500, 0), (0, 0))
        .unwrap();
    let sent = take(&mut q, t0);
    let sticks: Vec<(u16, i16)> = sent
        .iter()
        .map(|i| match i {
            Input::Controller { id, sticks, .. } => (*id, sticks[0]),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(sticks, [(1, 300), (0, 0), (1, 500)]);
    // A state already taken is not updated any more.
    q.send_controller(1, 3, button::B, 0, 0, (600, 0), (0, 0))
        .unwrap();
    assert_eq!(take(&mut q, t0).len(), 1);
}

#[test]
fn arrival_sends_a_neutral_state_too() {
    let mut q = queue(&sunshine());
    q.send_controller_arrival(17, 2, 2, 0xFFFF, capability::DUAL_TOUCHPAD)
        .unwrap();
    let sent = take(&mut q, Instant::now());
    assert_eq!(
        sent[0],
        Input::Arrival {
            id: 1,
            kind: 2,
            capabilities: 0x108,
            buttons: 0xFFFF
        }
    );
    assert!(matches!(
        sent[1],
        Input::Controller {
            id: 1,
            active: 2,
            buttons: 0,
            ..
        }
    ));
    let mut g = queue(&gfe());
    g.send_controller_arrival(0, 1, 1, 0, 0).unwrap();
    assert_eq!(take(&mut g, Instant::now()).len(), 1);
}

#[test]
fn identities_go_only_to_a_host_that_takes_them_and_the_grip_flag_is_kept() {
    use rubylight_protocol::controller_identity::ControllerIdentity;
    // The C button's flag is also a Steam Controller's left grip touch. A host
    // without ControllerIdentityVersion still needs the grip, so the queue
    // never clears the bit (nightjar#125); the desktop sends its own C press
    // only to a host that takes identities.
    let mut q = queue(&sunshine());
    q.send_controller_identity(1, 0x057E, 0x2069).unwrap();
    q.send_controller(1, 2, button::C | button::A, 0, 0, (0, 0), (0, 0))
        .unwrap();
    let sent = take(&mut q, Instant::now());
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(matches!(
        sent[0],
        Input::Controller {
            buttons,
            ..
        } if buttons == button::C | button::A
    ));

    q.set_controller_identity(true);
    q.send_controller_identity(17, 0x057E, 0x2069).unwrap();
    q.send_controller_arrival(1, 2, 3, 0xFFFF, 0).unwrap();
    q.send_controller(1, 2, button::C, 0, 0, (0, 0), (0, 0))
        .unwrap();
    let mut out = Vec::new();
    q.poll(Instant::now(), false, &mut out);
    let packets: Vec<Sent> = out.iter().map(|p| classify(&p.data)).collect();
    assert!(
        matches!(
            packets[0],
            Sent::Identity(ControllerIdentity {
                controller: 1,
                vendor: 0x057E,
                product: 0x2069
            })
        ),
        "{packets:?}"
    );
    assert!(matches!(packets[1], Sent::Host(Input::Arrival { id: 1, .. })), "{packets:?}");
    assert!(matches!(
        packets.last().unwrap(),
        Sent::Host(Input::Controller {
            buttons: button::C,
            ..
        })
    ));
}

#[test]
fn a_held_identity_goes_before_its_arrival() {
    let mut q = queue(&sunshine());
    q.set_controller_identity(true);
    let t0 = Instant::now();
    let mut out = Vec::new();
    q.send_controller_identity(0, 0x057E, 0x2069).unwrap();
    q.send_controller_arrival(0, 1, 3, 0xFFFF, 0).unwrap();
    q.poll(t0, true, &mut out);
    assert!(out.is_empty());
    q.poll(t0, false, &mut out);
    let packets: Vec<Sent> = out.iter().map(|p| classify(&p.data)).collect();
    assert!(matches!(packets[0], Sent::Identity(_)), "{packets:?}");
    assert!(matches!(packets[1], Sent::Host(Input::Arrival { id: 0, .. })), "{packets:?}");
}

#[test]
fn motion_keeps_the_latest_reading_and_parks_the_gyro_reliably() {
    let mut q = queue(&sunshine());
    let t0 = Instant::now();
    assert_eq!(
        q.send_controller_motion(0, 3, 0., 0., 0.),
        Err(InputError::Invalid)
    );
    assert_eq!(
        q.send_controller_motion(0, 0, 0., 0., 0.),
        Err(InputError::Invalid)
    );
    q.send_controller_motion(0, motion_type::ACCEL, 1., 2., 3.)
        .unwrap();
    q.send_controller_motion(0, motion_type::ACCEL, 4., 5., 6.)
        .unwrap();
    q.send_controller_motion(0, motion_type::GYRO, 0., 0., 0.)
        .unwrap();
    let mut out = Vec::new();
    q.poll(t0, false, &mut out);
    assert_eq!(out.len(), 2);
    assert_eq!(
        decode(&out[0].data).unwrap(),
        Input::Motion {
            id: 0,
            kind: 1,
            xyz: [4., 5., 6.]
        }
    );
    assert_eq!(out[0].delivery, Delivery::Unreliable);
    assert_eq!(out[0].channel, 0x20);
    assert_eq!(out[1].delivery, Delivery::Reliable);
}

/// A pen's roll goes as 0x5500000B only to a host that takes it; otherwise, and for a
/// roll out of range, the pen packet stays the stock one.
#[test]
fn the_barrel_roll_goes_only_where_it_is_offered() {
    let magics = |offered: bool, roll: u16| {
        let mut q = queue(&sunshine());
        q.set_pen_barrel_roll(offered);
        q.send_pen(touch_event::DOWN, 1, 0, 0.5, 0.5, 0.5, 0., 0., 0, 0, roll).unwrap();
        let mut out = Vec::new();
        q.poll(Instant::now(), false, &mut out);
        assert_eq!(out.len(), 1);
        (u32::from_le_bytes(out[0].data[4..8].try_into().unwrap()), out[0].data.len())
    };
    assert_eq!(magics(false, 90), (0x5500_0003, 36));
    assert_eq!(magics(true, pen::ROLL_UNKNOWN), (0x5500_0003, 36));
    assert_eq!(magics(true, 360), (0x5500_0003, 36));
    assert_eq!(magics(true, 90), (pen::PEN_BARREL_ROLL_MAGIC, 40));
}

#[test]
fn pen_moves_collapse_and_button_changes_go_reliably() {
    let mut q = queue(&sunshine());
    let t0 = Instant::now();
    let pen = |q: &mut InputQueue, event, buttons, x| {
        q.send_pen(event, 1, buttons, x, 0.5, 0.5, 0., 0., 0, 0, pen::ROLL_UNKNOWN)
    };
    pen(&mut q, touch_event::DOWN, 0, 0.1).unwrap();
    pen(&mut q, touch_event::MOVE, 0, 0.2).unwrap();
    pen(&mut q, touch_event::MOVE, 0, 0.3).unwrap();
    pen(&mut q, touch_event::MOVE, 1, 0.4).unwrap();
    let mut out = Vec::new();
    q.poll(t0, false, &mut out);
    let xs: Vec<(f32, Delivery)> = out
        .iter()
        .map(|p| match decode(&p.data).unwrap() {
            Input::Pen { x, .. } => (x, p.delivery),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(xs, [(0.1, Delivery::Reliable), (0.3, Delivery::Unreliable)]);
    let mut out = Vec::new();
    assert_eq!(
        q.poll(t0, false, &mut out),
        Some(t0 + MOTION_BATCHING_INTERVAL)
    );
    q.poll(t0 + MOTION_BATCHING_INTERVAL, false, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].delivery, Delivery::Reliable);
}

/// A move that changes the buttons, collapsed into a newer move with the same
/// buttons, still goes reliably: the newer one now carries the change.
#[test]
fn a_pen_button_change_stays_reliable_when_a_newer_move_replaces_it() {
    let mut q = queue(&sunshine());
    let t0 = Instant::now();
    let pen = |q: &mut InputQueue, event, buttons, x| {
        q.send_pen(event, 1, buttons, x, 0.5, 0.5, 0., 0., 0, 0, pen::ROLL_UNKNOWN)
    };
    pen(&mut q, touch_event::DOWN, 0, 0.1).unwrap();
    pen(&mut q, touch_event::MOVE, 1, 0.2).unwrap();
    pen(&mut q, touch_event::MOVE, 1, 0.3).unwrap();
    let mut out = Vec::new();
    q.poll(t0, false, &mut out);
    q.poll(t0 + MOTION_BATCHING_INTERVAL, false, &mut out);
    let xs: Vec<(f32, Delivery)> = out
        .iter()
        .map(|p| match decode(&p.data).unwrap() {
            Input::Pen { x, .. } => (x, p.delivery),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(xs, [(0.1, Delivery::Reliable), (0.3, Delivery::Reliable)]);
}

#[test]
fn touchpad_silence_cancels_buttons_and_queue_full_preserves_button_state() {
    let caps = HostCapabilities {
        feature_flags: feature::TOUCHPAD_EVENTS | feature::TOUCHPAD_FRAME_EVENTS,
        ..sunshine()
    };
    let contact = TouchpadContact {
        event: touch_event::MOVE,
        pointer_id: 1,
        x: 0.5,
        y: 0.5,
        pressure: 0.5,
    };

    // Holding touchpad frames while the host is silent emits a reliable CANCEL_ALL with buttons=0
    // on reconnect and resets tracked touchpad_buttons so a fresh click stays reliable.
    let mut q = queue(&caps);
    let t0 = Instant::now();
    q.send_touchpad_frame(&[contact], touchpad::BUTTON_PRIMARY, 100, 70)
        .unwrap();
    let mut out = Vec::new();
    q.poll(t0, true, &mut out);
    assert!(out.is_empty());
    q.poll(t0, false, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].delivery, Delivery::Reliable);
    assert_eq!(
        u32::from_le_bytes(out[0].data[4..8].try_into().unwrap()),
        magic::TOUCHPAD
    );
    assert_eq!(out[0].data[8], touch_event::CANCEL_ALL);
    assert_eq!(out[0].data[9], 0, "buttons cleared on cancel");

    // Next frame pressing BUTTON_PRIMARY must be treated as a change (Reliable), not Unreliable.
    out.clear();
    q.send_touchpad_frame(&[contact], touchpad::BUTTON_PRIMARY, 100, 70)
        .unwrap();
    q.poll(t0, false, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].delivery, Delivery::Reliable);

    // QueueFull on a button-press touchpad frame must not clobber touchpad_buttons, while an
    // empty release frame uses the release reserve above MAX_QUEUED.
    let mut q2 = queue(&caps);
    for i in 0..MAX_QUEUED {
        q2.send_mouse_button(1, i % 2 == 0).unwrap();
    }
    assert_eq!(
        q2.send_touchpad_frame(&[contact], touchpad::BUTTON_PRIMARY, 100, 70),
        Err(InputError::QueueFull)
    );
    q2.send_touchpad_frame(&[], 0, 100, 70)
        .expect("empty touchpad release fits the release reserve");
    let mut drained = Vec::new();
    q2.drain(t0, false, &mut drained);
    drained.clear();
    // Because the failed press did not advance touchpad_buttons, retrying it stays Reliable.
    q2.send_touchpad_frame(&[contact], touchpad::BUTTON_PRIMARY, 100, 70)
        .unwrap();
    q2.poll(t0, false, &mut drained);
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].delivery, Delivery::Reliable);
}

#[test]
fn touchpad_frame_only_host_never_gets_0x55000009_on_silence_cancel() {
    let caps = HostCapabilities {
        feature_flags: feature::TOUCHPAD_FRAME_EVENTS,
        ..sunshine()
    };
    let mut q = queue(&caps);
    let t0 = Instant::now();
    let up = TouchpadContact {
        event: touch_event::UP,
        pointer_id: 1,
        x: 0.5,
        y: 0.5,
        pressure: 0.0,
    };
    q.send_touchpad_frame(&[up], touchpad::BUTTON_PRIMARY, 100, 70)
        .unwrap();
    let mut out = Vec::new();
    q.poll(t0, true, &mut out);
    assert!(out.is_empty());
    q.poll(t0, false, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].delivery, Delivery::Reliable);
    assert_eq!(
        u32::from_le_bytes(out[0].data[4..8].try_into().unwrap()),
        magic::TOUCHPAD_FRAME
    );
    assert_eq!(out[0].data[8], 1, "single synthetic CANCEL_ALL contact");
    assert_eq!(out[0].data[9], 0, "buttons cleared on cancel");
    assert_eq!(out[0].data[16], touch_event::CANCEL_ALL);
}

#[test]
fn touchpad_frame_finger_up_with_button_held_uses_release_reserve() {
    let caps = HostCapabilities {
        feature_flags: feature::TOUCHPAD_FRAME_EVENTS,
        ..sunshine()
    };
    let mut q = queue(&caps);
    for i in 0..MAX_QUEUED {
        q.send_mouse_button(1, i % 2 == 0).unwrap();
    }
    let move_contact = TouchpadContact {
        event: touch_event::MOVE,
        pointer_id: 1,
        x: 0.5,
        y: 0.5,
        pressure: 0.5,
    };
    assert_eq!(
        q.send_touchpad_frame(&[move_contact], 0, 100, 70),
        Err(InputError::QueueFull),
        "moves without a held button must not consume release reserve"
    );
    let up_contact = TouchpadContact {
        event: touch_event::UP,
        ..move_contact
    };
    q.send_touchpad_frame(&[up_contact], touchpad::BUTTON_PRIMARY, 100, 70)
        .expect("finger release fits the release reserve even while a button stays held");
}

#[test]
fn controller_coalescing_does_not_overwrite_changed_active_mask() {
    let mut q = queue(&sunshine());
    let t0 = Instant::now();
    // Unplug controller 0 (active_mask = 0), then re-plug it (active_mask = 1) before polling.
    q.send_controller(0, 0, 0, 0, 0, (0, 0), (0, 0)).unwrap();
    q.send_controller(0, 1, 0, 0, 0, (10, 20), (0, 0)).unwrap();
    let sent = take(&mut q, t0);
    let masks: Vec<(u16, i16)> = sent
        .iter()
        .map(|i| match i {
            Input::Controller { active, sticks, .. } => (*active, sticks[0]),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(masks, [(0, 0), (1, 10)]);
}
