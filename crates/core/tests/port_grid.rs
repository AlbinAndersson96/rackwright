use rackwright_core::ids::PortId;
use rackwright_core::port_grid::{
    Cell, GridSize, PortPlacementError, PushDir, push_direction, push_for_drop,
};
use uuid::Uuid;

const ONE_U: GridSize = GridSize { rows: 1, cols: 5 };
const TWO_U: GridSize = GridSize { rows: 2, cols: 5 };

fn at(n: u128, row: u32, col: u32) -> (PortId, Cell) {
    (PortId(Uuid::from_u128(n)), Cell { row, col })
}

fn cells(result: Vec<(PortId, Cell)>) -> Vec<(u32, u32)> {
    result.into_iter().map(|(_, c)| (c.row, c.col)).collect()
}

#[test]
fn direction_from_cursor_offset() {
    assert_eq!(push_direction(-0.3, 0.1), PushDir::Right); // cursor left → push right
    assert_eq!(push_direction(0.3, 0.1), PushDir::Left);
    assert_eq!(push_direction(0.1, -0.3), PushDir::Up); // cursor below → push up
    assert_eq!(push_direction(0.1, 0.3), PushDir::Down);
}

#[test]
fn direction_ties_go_horizontal_and_centre_goes_right() {
    assert_eq!(push_direction(0.2, 0.2), PushDir::Left);
    assert_eq!(push_direction(-0.2, 0.2), PushDir::Right);
    assert_eq!(push_direction(0.0, 0.0), PushDir::Right);
}

#[test]
fn free_target_moves_nothing() {
    let occ = [at(1, 0, 1)];
    let r = push_for_drop(&occ, Cell { row: 0, col: 3 }, PushDir::Right, ONE_U).unwrap();
    assert_eq!(cells(r), vec![(0, 1)]);
}

#[test]
fn pushes_right_with_cascade() {
    let occ = [at(1, 0, 1), at(2, 0, 2), at(3, 0, 4)];
    let r = push_for_drop(&occ, Cell { row: 0, col: 1 }, PushDir::Right, ONE_U).unwrap();
    assert_eq!(cells(r), vec![(0, 2), (0, 3), (0, 4)]);
}

#[test]
fn pushes_left() {
    let occ = [at(1, 0, 1)];
    let r = push_for_drop(&occ, Cell { row: 0, col: 1 }, PushDir::Left, ONE_U).unwrap();
    assert_eq!(cells(r), vec![(0, 0)]);
}

#[test]
fn pushes_up_and_down() {
    let occ = [at(1, 0, 2)];
    let r = push_for_drop(&occ, Cell { row: 0, col: 2 }, PushDir::Up, TWO_U).unwrap();
    assert_eq!(cells(r), vec![(1, 2)]);
    let occ = [at(1, 1, 2)];
    let r = push_for_drop(&occ, Cell { row: 1, col: 2 }, PushDir::Down, TWO_U).unwrap();
    assert_eq!(cells(r), vec![(0, 2)]);
}

#[test]
fn rejects_push_off_any_edge() {
    let occ = [at(1, 0, 3), at(2, 0, 4)];
    assert_eq!(
        push_for_drop(&occ, Cell { row: 0, col: 3 }, PushDir::Right, ONE_U),
        Err(PortPlacementError::NoRoom)
    );
    let occ = [at(1, 0, 0)];
    assert_eq!(
        push_for_drop(&occ, Cell { row: 0, col: 0 }, PushDir::Left, ONE_U),
        Err(PortPlacementError::NoRoom)
    );
    assert_eq!(
        push_for_drop(&occ, Cell { row: 0, col: 0 }, PushDir::Down, ONE_U),
        Err(PortPlacementError::NoRoom)
    );
    assert_eq!(
        push_for_drop(&occ, Cell { row: 0, col: 0 }, PushDir::Up, ONE_U),
        Err(PortPlacementError::NoRoom)
    );
}

#[test]
fn rejects_target_outside_grid() {
    assert_eq!(
        push_for_drop(&[], Cell { row: 1, col: 0 }, PushDir::Right, ONE_U),
        Err(PortPlacementError::OutOfGrid)
    );
    assert_eq!(
        push_for_drop(&[], Cell { row: 0, col: 5 }, PushDir::Right, ONE_U),
        Err(PortPlacementError::OutOfGrid)
    );
}
