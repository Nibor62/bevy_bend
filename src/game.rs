use bevy::{
    core::Time,
    ecs::{
        event::EventReader,
        system::{
            Query,
            Res,
        },
        world::Mut,
    },
    input::{
        Input,
        keyboard::KeyCode,
        mouse::MouseMotion,
    },
    math::{
        Quat,
        Vec3,
    },
    transform::components::Transform,
};

use std::ops::Add;

pub struct Moving();

pub fn moving(
    time: Res<Time>,
    mut event_mouse_motion: EventReader<MouseMotion>,
    keyboard_input : Res<Input<KeyCode>>,
    mut query: Query<(&mut Transform, &mut Moving)>,
) {
    const MOUSE_SENSITIVITY : f32 = 0.001;
    const MOVE_SENSITIVITY  : f32 = 5.;

    // directly use a transform
    // see local_z in order to move forward
    for (mut transform, _) in query.iter_mut() {
        let mut rot = Vec3::ZERO;
        for event in event_mouse_motion.iter() {
            rot[0] += -event.delta.x * MOUSE_SENSITIVITY;
            rot[1] += -event.delta.y * MOUSE_SENSITIVITY;
        }

        // Mindlessly copied that from some oneline solution to avoid weird
        //  rotation things.
        // Should investigate the math behind it someday
        // EDIT : in fact is pretty obvious
        //        rotation are relative to the previous
        //        so first rotate on y (up down)
        //        then relatively applied the curent rot
        //        then relatively apply x rot (right left)
        //
        //        In the end it boil downs to righ-left rot must be relative to updown
        //        and not the opposite
        transform.rotation = Quat::from_rotation_y(rot[0])
            * transform.rotation
            * Quat::from_rotation_x(rot[1])
        ;

        //let oui : &[(_, fn(&Mut<Transform>) -> Vec3)] = &[
        let motions : &[(_, fn(&Mut<Transform>) -> Vec3)] = &[
            (KeyCode::Up,     |t| -t.local_z()),
            (KeyCode::Down,   |t|  t.local_z()),
            (KeyCode::Right,  |t|  t.local_x()),
            (KeyCode::Left,   |t| -t.local_x()),
            (KeyCode::Space,  |t|  t.local_y()),
            (KeyCode::LShift, |t| -t.local_y()),
        ];

        // trully better than a for + if ?
        let pos : Vec3 = motions.iter()
            //.filter(|(keycode, _)| keyboard_input.just_pressed(*keycode))
            .filter(|(keycode, _)| keyboard_input.pressed(*keycode))
            .map( |(_, transform_callback)| transform_callback(&transform) as Vec3 )
            .reduce(Vec3::add) // sum does not work (needed to collect + iter for some reason)
            .unwrap_or(Vec3::ZERO)
            .normalize_or_zero() // just want a direction
        ;

        transform.translation += pos * MOVE_SENSITIVITY * time.delta_seconds();
    }
}
