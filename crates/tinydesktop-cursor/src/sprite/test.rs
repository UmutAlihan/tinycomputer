//! Tests for the cursor sprite and its PNG encoding.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::png::{adler32, crc32};
use super::{PULSE_FRAMES, SIZE, Sprite, png};

fn alpha(sprite: &Sprite, frame: usize, x: u32, y: u32) -> u8 {
    let side = sprite.pixels();
    sprite.frames[frame][usize::try_from((y * side + x) * 4 + 3).unwrap()]
}

#[test]
fn the_sprite_has_a_resting_frame_and_the_pulse() {
    let sprite = Sprite::render(1);
    assert_eq!(sprite.frames.len(), PULSE_FRAMES + 1);
    assert_eq!(sprite.pixels(), SIZE);
    for frame in &sprite.frames {
        assert_eq!(frame.len(), usize::try_from(SIZE * SIZE * 4).unwrap());
    }
    assert_eq!(Sprite::render(9).scale, 4, "scale is clamped");
}

#[test]
fn the_arrow_is_drawn_from_the_tip_down_and_right_and_nothing_else_at_rest() {
    let sprite = Sprite::render(2);
    let tip = Sprite::hotspot();
    let (x, y) = (2 * (super::SIZE / 2), 2 * (super::SIZE / 2));
    assert!((tip.x - 32.0).abs() < f64::EPSILON);
    assert_eq!(
        alpha(&sprite, 0, x + 6, y + 20),
        255,
        "inside the arrow body"
    );
    let body = usize::try_from(((y + 20) * sprite.pixels() + x + 6) * 4).unwrap();
    assert_eq!(
        &sprite.frames[0][body..body + 3],
        &[124, 92, 255],
        "violet fill"
    );
    assert_eq!(alpha(&sprite, 0, 2, 2), 0, "the corners are clear");
    assert_eq!(
        alpha(&sprite, 0, x - 20, y - 20),
        0,
        "nothing up and left of the tip"
    );
}

#[test]
fn the_pulse_ring_grows_and_fades() {
    let sprite = Sprite::render(1);
    let (x, y) = (SIZE / 2, SIZE / 2);
    // Up and left of the tip only the ring can be drawn.
    let ring_at = |frame: usize| (1..32).rev().find(|r| alpha(&sprite, frame, x - r, y) > 0);
    let early = ring_at(1).unwrap();
    let late = ring_at(PULSE_FRAMES - 2).unwrap();
    assert!(late > early, "{early} -> {late}");
    // Past five points left of the tip only the ring is drawn.
    let peak = |frame: usize| {
        (5..32)
            .map(|r| alpha(&sprite, frame, x - r, y))
            .max()
            .unwrap()
    };
    assert!(peak(PULSE_FRAMES) < peak(1));
}

#[test]
fn pulse_progress_picks_a_pulse_frame() {
    assert_eq!(Sprite::frame_for(None), 0);
    assert_eq!(Sprite::frame_for(Some(0.0)), 1);
    assert_eq!(Sprite::frame_for(Some(0.5)), 7);
    assert_eq!(Sprite::frame_for(Some(0.999)), PULSE_FRAMES);
    assert_eq!(Sprite::frame_for(Some(7.0)), PULSE_FRAMES);
}

#[test]
fn bgra_is_premultiplied_and_swizzled() {
    let sprite = Sprite {
        scale: 1,
        frames: vec![vec![200, 100, 50, 128, 10, 20, 30, 0]],
    };
    assert_eq!(
        sprite.premultiplied_bgra(0),
        vec![25, 50, 100, 128, 0, 0, 0, 0]
    );
    assert!(sprite.premultiplied_bgra(3).is_empty());
}

#[test]
fn png_output_is_a_valid_stream() {
    let rgba = [255_u8, 0, 0, 255, 0, 255, 0, 128];
    let encoded = png(2, 1, &rgba);
    assert_eq!(&encoded[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&encoded[12..16], b"IHDR");
    assert_eq!(&encoded[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
    assert_eq!(&encoded[encoded.len() - 8..encoded.len() - 4], b"IEND");
    // The IEND chunk's CRC is a known constant.
    assert_eq!(&encoded[encoded.len() - 4..], &[0xAE, 0x42, 0x60, 0x82]);
    assert!(png(3, 1, &rgba).is_empty(), "wrong size is refused");
    assert!(png(0, 1, &[]).is_empty());
    let big = vec![7_u8; 70_000 * 4];
    assert!(
        png(70_000, 1, &big).len() > big.len(),
        "several stored blocks"
    );
}

#[test]
fn checksums_match_their_references() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
}
