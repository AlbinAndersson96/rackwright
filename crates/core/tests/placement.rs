use rackwright_core::ids::DeviceId;
use rackwright_core::placement::{
    Occupant, PlacementError, clamp_bottom, push_for_drop, push_for_growth,
};
use uuid::Uuid;

fn occ(n: u128, bottom_u: u32, height_u: u32) -> Occupant {
    Occupant {
        id: DeviceId(Uuid::from_u128(n)),
        bottom_u,
        height_u,
    }
}

fn bottoms(result: Vec<(DeviceId, u32)>) -> Vec<u32> {
    result.into_iter().map(|(_, b)| b).collect()
}

#[test]
fn clamps_into_the_rack() {
    assert_eq!(clamp_bottom(0, 2, 42), Ok(1));
    assert_eq!(clamp_bottom(42, 2, 42), Ok(41));
    assert_eq!(clamp_bottom(10, 2, 42), Ok(10));
    assert_eq!(clamp_bottom(1, 42, 42), Ok(1));
    assert_eq!(clamp_bottom(1, 43, 42), Err(PlacementError::TooTall));
}

#[test]
fn free_slot_moves_nothing() {
    let others = [occ(1, 1, 2)];
    assert_eq!(bottoms(push_for_drop(&others, 10, 2, 42).unwrap()), vec![1]);
}

#[test]
fn device_above_centre_is_pushed_up() {
    // Drop occupies U10-U11; A at U11 has its centre above → moves to U12.
    let others = [occ(1, 11, 1)];
    assert_eq!(
        bottoms(push_for_drop(&others, 10, 2, 42).unwrap()),
        vec![12]
    );
}

#[test]
fn device_below_centre_is_pushed_down() {
    // Drop occupies U10-U11; A at U10 has its centre below → moves to U9.
    let others = [occ(1, 10, 1)];
    assert_eq!(bottoms(push_for_drop(&others, 10, 2, 42).unwrap()), vec![9]);
}

#[test]
fn device_with_equal_centre_is_pushed_down() {
    // Drop 2U onto an identical 2U device at U10 → it moves to U8-U9.
    let others = [occ(1, 10, 2)];
    assert_eq!(bottoms(push_for_drop(&others, 10, 2, 42).unwrap()), vec![8]);
}

#[test]
fn upward_push_cascades() {
    let others = [occ(1, 11, 1), occ(2, 12, 2), occ(3, 20, 1)];
    assert_eq!(
        bottoms(push_for_drop(&others, 10, 2, 42).unwrap()),
        vec![12, 13, 20]
    );
}

#[test]
fn downward_push_cascades() {
    // Drop occupies U9-U10. A (U9) → U8, which pushes B (U7-U8) → U6-U7.
    let others = [occ(1, 9, 1), occ(2, 7, 2)];
    assert_eq!(
        bottoms(push_for_drop(&others, 9, 2, 42).unwrap()),
        vec![8, 6]
    );
}

#[test]
fn rejects_when_upward_push_leaves_the_rack() {
    let others = [occ(1, 11, 2)];
    assert_eq!(
        push_for_drop(&others, 10, 2, 12),
        Err(PlacementError::NoRoomAbove)
    );
}

#[test]
fn rejects_when_downward_push_passes_u1_even_if_space_above() {
    let others = [occ(1, 1, 1)];
    assert_eq!(
        push_for_drop(&others, 1, 1, 42),
        Err(PlacementError::NoRoomBelow)
    );
}

#[test]
fn growth_pushes_only_devices_above() {
    // Device at U10 grows to 3U (U10-U12).
    let others = [occ(1, 12, 1), occ(2, 13, 1), occ(3, 5, 1)];
    assert_eq!(
        bottoms(push_for_growth(&others, 10, 3, 42).unwrap()),
        vec![13, 14, 5]
    );
}

#[test]
fn growth_rejects_when_push_leaves_the_rack() {
    let others = [occ(1, 12, 2)];
    assert_eq!(
        push_for_growth(&others, 10, 3, 13),
        Err(PlacementError::NoRoomAbove)
    );
}
