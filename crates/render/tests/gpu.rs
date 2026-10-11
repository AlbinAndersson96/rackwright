//! Pixel tests on a real (possibly software) GPU adapter. Skipped when none is available.

use glam::{Vec2, Vec3};
use rackwright_render::camera::{FOV_Y, MAX_DISTANCE_MM, OrbitCamera};
use rackwright_render::gpu::{FrameStatus, Renderer};
use rackwright_render::layout::{Aabb, FaceRect, LABEL_OFFSET_MM};
use rackwright_render::scene::{BACKGROUND, BoxInstance, Floor, Label, Scene, TubeInstance};
use rackwright_render::text::TextRasterizer;

const SIZE: u32 = 64;

/// Fills the whole label texture with the label colour.
struct SolidText;

impl TextRasterizer for SolidText {
    fn rasterize(&mut self, _text: &str, width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        color.repeat((width * height) as usize)
    }
}

/// Serialises the GPU tests: creating Vulkan devices concurrently in one process occasionally
/// segfaults inside Mesa's llvmpipe driver (reproduced with plain wgpu, no app code).
static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn renderer() -> Option<Renderer> {
    match pollster::block_on(Renderer::new_offscreen(SIZE, SIZE)) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

/// Looks straight at the origin from 1 m; a ±100 mm box spans pixels 24..40.
fn camera() -> OrbitCamera {
    OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        distance: 1000.0,
    }
}

fn red_box() -> BoxInstance {
    BoxInstance {
        aabb: Aabb::new(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 100.0, 0.0),
        ),
        color: [1.0, 0.0, 0.0, 1.0],
    }
}

fn draw(r: &mut Renderer, scene: &Scene) -> Vec<u8> {
    r.render(scene, &camera(), &mut SolidText).unwrap();
    r.read_pixels().unwrap()
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn background() -> [u8; 4] {
    [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b, 255]
}

#[test]
fn draws_a_shaded_box_on_the_background() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let scene = Scene {
        opaque: vec![red_box()],
        ..Default::default()
    };
    let p = draw(&mut r, &scene);
    let [red, green, blue, _] = pixel(&p, 32, 32);
    assert!(
        (184..=200).contains(&red) && green < 8 && blue < 8,
        "front face {red},{green},{blue}"
    );
    assert_eq!(pixel(&p, 2, 2), background());
}

#[test]
fn box_edges_are_anti_aliased() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    // Put the right edge a quarter of the way into pixel column 40: only some samples cover it.
    let mm_per_px = 2.0 * camera().distance * (FOV_Y * 0.5).tan() / SIZE as f32;
    let mut aabb = red_box().aabb;
    aabb.max.x = (40.25 - SIZE as f32 / 2.0) * mm_per_px;
    let scene = Scene {
        opaque: vec![BoxInstance { aabb, ..red_box() }],
        ..Default::default()
    };
    let p = draw(&mut r, &scene);
    let row: Vec<[u8; 4]> = (36..44).map(|x| pixel(&p, x, 32)).collect();
    assert!(
        row.iter().any(|[_, green, _, _]| (20..220).contains(green)),
        "no partly covered pixel at the edge: {row:?}"
    );
}

#[test]
fn selection_draws_a_glow_just_outside_the_box() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let plain = Scene {
        opaque: vec![red_box()],
        ..Default::default()
    };
    assert_eq!(pixel(&draw(&mut r, &plain), 22, 32), background());
    let selected = Scene {
        selected: vec![red_box()],
        ..plain
    };
    let p = draw(&mut r, &selected);
    let [red, green, blue, _] = pixel(&p, 22, 32);
    assert!(
        red > 240 && (150..220).contains(&green) && blue < 120,
        "glow {red},{green},{blue}"
    );
    assert_eq!(
        pixel(&p, 10, 32),
        background(),
        "glow fades within a few pixels"
    );
}

#[test]
fn selection_glow_follows_sub_pixel_edges() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let mm_per_px = 2.0 * camera().distance * (FOV_Y * 0.5).tan() / SIZE as f32;
    // Both right edges stop short of pixel column 40's centre, so an aliased mask can't
    // tell them apart.
    let mut glow_for = |edge_px: f32| {
        let mut aabb = red_box().aabb;
        aabb.max.x = (edge_px - SIZE as f32 / 2.0) * mm_per_px;
        let selected = BoxInstance { aabb, ..red_box() };
        let scene = Scene {
            opaque: vec![selected],
            selected: vec![selected],
            ..Default::default()
        };
        let p = draw(&mut r, &scene);
        (40..44).map(|x| pixel(&p, x, 32)).collect::<Vec<_>>()
    };
    let near = glow_for(40.1);
    let far = glow_for(40.45);
    assert_ne!(
        near, far,
        "the glow ignores where the edge lies within a pixel"
    );
}

#[test]
fn labels_show_their_texture() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let label = Label {
        text: "DMM".into(),
        rect: FaceRect {
            min: Vec2::splat(-20.0),
            max: Vec2::splat(20.0),
            z: 1.0,
        },
        color: [0, 0, 255, 255],
    };
    let scene = Scene {
        opaque: vec![red_box()],
        labels: vec![label],
        ..Default::default()
    };
    assert_eq!(pixel(&draw(&mut r, &scene), 32, 32), [0, 0, 255, 255]);
}

#[test]
fn ghost_is_blended_over_the_scene() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let ghost = BoxInstance {
        aabb: Aabb::new(
            Vec3::new(-300.0, -20.0, 50.0),
            Vec3::new(-150.0, 20.0, 60.0),
        ),
        color: [0.9, 0.15, 0.15, 0.45],
    };
    let scene = Scene {
        translucent: vec![ghost],
        ..Default::default()
    };
    let [red, green, blue, _] = pixel(&draw(&mut r, &scene), 15, 32);
    assert!(
        red > green + 40 && green > 100 && blue > 100,
        "reddish tint {red},{green},{blue}"
    );
}

#[test]
fn resize_changes_the_output_size() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    r.resize(32, 16);
    assert_eq!(r.size(), (32, 16));
    r.render(&Scene::default(), &camera(), &mut SolidText)
        .unwrap();
    assert_eq!(r.read_pixels().unwrap().len(), 32 * 16 * 4);
}

#[test]
fn zero_size_is_clamped_to_one_pixel() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    r.resize(0, 0);
    assert_eq!(r.size(), (1, 1));
    r.render(&Scene::default(), &camera(), &mut SolidText)
        .unwrap();
    assert_eq!(r.read_pixels().unwrap().len(), 4);
}

#[test]
fn offscreen_frames_report_that_they_were_drawn() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    assert_eq!(
        r.render(&Scene::default(), &camera(), &mut SolidText),
        Ok(FrameStatus::Drawn)
    );
}

#[test]
fn resize_is_clamped_to_the_device_texture_limit() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    r.resize(100_000, 10);
    assert_eq!(r.size(), (r.max_dimension(), 10));
    assert_eq!(
        r.render(&Scene::default(), &camera(), &mut SolidText),
        Ok(FrameStatus::Drawn)
    );
}

#[test]
fn touching_boxes_of_one_colour_show_a_seam() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let grey = [0.6, 0.6, 0.6, 1.0];
    let lower = BoxInstance {
        aabb: Aabb::new(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 0.0, 0.0),
        ),
        color: grey,
    };
    let upper = BoxInstance {
        aabb: Aabb::new(Vec3::new(-100.0, 0.0, -100.0), Vec3::new(100.0, 100.0, 0.0)),
        color: grey,
    };
    let scene = Scene {
        opaque: vec![lower, upper],
        ..Default::default()
    };
    let p = draw(&mut r, &scene);
    let face = pixel(&p, 32, 27)[0];
    let seam = pixel(&p, 32, 31)[0].min(pixel(&p, 32, 32)[0]);
    assert!(
        u32::from(seam) + 20 < u32::from(face),
        "seam {seam} is not darker than the face {face}"
    );
}

/// The left half of every label is opaque white, the right half fully transparent black, as a
/// browser canvas returns it.
struct HalfWhiteText;

impl TextRasterizer for HalfWhiteText {
    fn rasterize(&mut self, _text: &str, width: u32, height: u32, _color: [u8; 4]) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| {
                if i % width < width / 2 {
                    [255; 4]
                } else {
                    [0; 4]
                }
            })
            .collect()
    }
}

#[test]
fn transparent_label_texels_do_not_darken_the_face() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let light = BoxInstance {
        color: [0.9, 0.9, 0.9, 1.0],
        ..red_box()
    };
    let label = Label {
        text: "PSU".into(),
        rect: FaceRect {
            min: Vec2::splat(-100.0),
            max: Vec2::splat(100.0),
            z: 1.0,
        },
        color: [255, 255, 255, 255],
    };
    let scene = Scene {
        opaque: vec![light],
        labels: vec![label],
        ..Default::default()
    };
    r.render(&scene, &camera(), &mut HalfWhiteText).unwrap();
    let p = r.read_pixels().unwrap();
    let face = pixel(&p, 38, 32)[0];
    for x in 28..37 {
        let [red, ..] = pixel(&p, x, 32);
        assert!(
            u32::from(red) + 3 >= u32::from(face),
            "pixel {x} is {red}, darker than both the face ({face}) and the white text"
        );
    }
}

#[test]
fn labels_stay_in_front_of_their_face_at_maximum_zoom_out() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let wall = BoxInstance {
        aabb: Aabb::new(
            Vec3::new(-5000.0, -5000.0, -100.0),
            Vec3::new(5000.0, 5000.0, 0.0),
        ),
        color: [1.0, 0.0, 0.0, 1.0],
    };
    let label = Label {
        text: "RACK".into(),
        rect: FaceRect {
            min: Vec2::splat(-3000.0),
            max: Vec2::splat(3000.0),
            z: LABEL_OFFSET_MM,
        },
        color: [0, 0, 255, 255],
    };
    let scene = Scene {
        opaque: vec![wall],
        labels: vec![label],
        ..Default::default()
    };
    let far = OrbitCamera {
        distance: MAX_DISTANCE_MM,
        ..camera()
    };
    r.render(&scene, &far, &mut SolidText).unwrap();
    let p = r.read_pixels().unwrap();
    for (x, y) in [(32, 32), (30, 30), (34, 33), (31, 34)] {
        assert_eq!(pixel(&p, x, y), [0, 0, 255, 255], "label hidden at {x},{y}");
    }
}

#[test]
fn the_floor_shows_grid_lines_that_fade_out_at_its_edge() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let scene = Scene {
        floor: Some(Floor {
            min: Vec2::splat(-1500.0),
            max: Vec2::splat(1500.0),
        }),
        ..Default::default()
    };
    // Looking down at the origin, where the x = 0 and z = 0 grid lines cross.
    let above = OrbitCamera {
        pitch: 1.2,
        distance: 2500.0,
        ..camera()
    };
    r.render(&scene, &above, &mut SolidText).unwrap();
    let p = r.read_pixels().unwrap();
    let [red, ..] = pixel(&p, 32, 32);
    assert!(
        u32::from(red) + 10 < u32::from(BACKGROUND.r),
        "no grid line at the origin: {red}"
    );
    assert_eq!(
        pixel(&p, 1, 1),
        background(),
        "the floor fades out at its edge"
    );
    // From below, the floor is not drawn.
    let below = OrbitCamera {
        pitch: -1.2,
        ..above
    };
    r.render(&scene, &below, &mut SolidText).unwrap();
    assert_eq!(pixel(&r.read_pixels().unwrap(), 32, 32), background());
}

/// A green tube 100 mm thick (about 8 px) from `a` to `b`.
fn tube(a: Vec3, b: Vec3) -> TubeInstance {
    TubeInstance {
        a,
        b,
        radius: 50.0,
        color: [0.0, 1.0, 0.0, 1.0],
    }
}

fn is_green([red, green, blue, _]: [u8; 4]) -> bool {
    green > 100 && red < 40 && blue < 40
}

#[test]
fn tubes_are_drawn_in_any_direction() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let (left, right) = (Vec3::new(-300.0, 0.0, 0.0), Vec3::new(300.0, 0.0, 0.0));
    let (down, up) = (Vec3::new(0.0, -300.0, 0.0), Vec3::new(0.0, 300.0, 0.0));
    let (back, front) = (Vec3::new(0.0, 0.0, -300.0), Vec3::new(0.0, 0.0, 300.0));
    for (a, b) in [
        (left, right),
        (right, left),
        (down, up),
        (up, down),
        (back, front),
    ] {
        let scene = Scene {
            tubes: vec![tube(a, b)],
            ..Default::default()
        };
        let p = draw(&mut r, &scene);
        assert!(
            is_green(pixel(&p, 32, 32)),
            "{a} → {b}: {:?}",
            pixel(&p, 32, 32)
        );
        assert_eq!(pixel(&p, 12, 12), background(), "{a} → {b}");
    }
}

#[test]
fn tubes_are_shaded_round() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let scene = Scene {
        tubes: vec![tube(
            Vec3::new(-300.0, 0.0, 0.0),
            Vec3::new(300.0, 0.0, 0.0),
        )],
        ..Default::default()
    };
    let p = draw(&mut r, &scene);
    // The light comes from above: the upper side is brighter than the lower side.
    let (upper, lower) = (pixel(&p, 32, 30)[1], pixel(&p, 32, 34)[1]);
    assert!(upper > lower, "upper {upper}, lower {lower}");
    assert_eq!(pixel(&p, 32, 20), background());
}

#[test]
fn a_selected_tube_glows() {
    let _gpu = gpu_lock();
    let Some(mut r) = renderer() else { return };
    let t = tube(Vec3::new(-300.0, 0.0, 0.0), Vec3::new(300.0, 0.0, 0.0));
    let scene = Scene {
        tubes: vec![t],
        selected_tubes: vec![t],
        ..Default::default()
    };
    let p = draw(&mut r, &scene);
    let column: Vec<[u8; 4]> = (22..30).map(|y| pixel(&p, 32, y)).collect();
    assert!(
        column
            .iter()
            .any(|&[red, green, blue, _]| red > 240 && (150..220).contains(&green) && blue < 120),
        "no glow above the tube: {column:?}"
    );
}
