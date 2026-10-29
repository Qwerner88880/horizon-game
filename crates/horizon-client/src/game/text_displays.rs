//! Server text and block displays: metadata, interpolation, billboard
//! transforms, world text and block models.
use super::Game;
use horizon_client::{
    hud::{HudBatch, rgba},
    mesher::Vertex,
    text::Text,
    text_display::{Align, TextDisplayData},
};
use horizon_protocol::entity_data::Value;
use horizon_world::registry::BlockStateId;

pub(super) struct RemoteTextDisplay {
    pub data: TextDisplayData,
    from: TextDisplayData,
    shown: TextDisplayData,
    position: glam::DVec3,
    previous: glam::DVec3,
    target: glam::DVec3,
    rotation: [f32; 2],
    old_rotation: [f32; 2],
    target_rotation: [f32; 2],
    steps: i32,
    age: i64,
    start: i64,
    duration: i32,
    dirty: bool,
    start_dirty: bool,
    duration_dirty: bool,
    initialized: bool,
    /// `Display.lastProgress`: the progress the last frame drew, where a new
    /// interpolation starts from.
    last_progress: f32,
    /// `TextDisplay.clientDisplayCache`: the split lines, until the render
    /// state changes.
    lines: Option<Lines>,
    /// A block display's block (`BlockDisplay.DATA_BLOCK_STATE_ID`); None
    /// for a text display.
    pub block: Option<BlockStateId>,
    /// An item display's stack (`ItemDisplay.DATA_ITEM_STACK_ID`); None for
    /// the other kinds, and for an item display until its data says.
    pub item: Option<horizon_client::items::ItemStack>,
    /// The mob that carries it (`SetPassengers`, capability `displays`):
    /// while it rides, the carrier's position is the display's.
    pub riding: Option<u32>,
}

/// `TextDisplay.CachedInfo`: each line and its width, and the widest.
struct Lines {
    lines: Vec<(Text, f32)>,
    width: f32,
}

/// `Display.RENDER_STATE_IDS` and `TextDisplay.TEXT_RENDER_STATE_IDS`: the
/// data a render state is built from (not the view range, size or glow).
fn renders(id: u8) -> bool {
    matches!(id, 11..=16 | 18 | 19 | 23..=27)
}

impl RemoteTextDisplay {
    pub fn new(position: [f64; 3], yaw: f32) -> Self {
        Self {
            data: Default::default(),
            from: Default::default(),
            shown: Default::default(),
            position: position.into(),
            previous: position.into(),
            target: position.into(),
            rotation: [yaw, 0.0],
            old_rotation: [yaw, 0.0],
            target_rotation: [yaw, 0.0],
            steps: 0,
            age: 0,
            start: 0,
            duration: 0,
            dirty: true,
            start_dirty: false,
            duration_dirty: false,
            initialized: false,
            last_progress: 0.0,
            lines: None,
            block: None,
            item: None,
            riding: None,
        }
    }
    /// A block display, showing air until its data says otherwise.
    pub fn new_block(position: [f64; 3], yaw: f32) -> Self {
        Self {
            block: Some(BlockStateId(0)),
            ..Self::new(position, yaw)
        }
    }
    /// An item display, drawing nothing until its data brings a stack.
    pub fn new_item(position: [f64; 3], yaw: f32) -> Self {
        Self::new(position, yaw)
    }
    fn progress(&self, partial: f32) -> f32 {
        if self.duration <= 0 {
            1.0
        } else {
            (((self.age - self.start) as f32 + partial) / self.duration as f32).clamp(0.0, 1.0)
        }
    }
    pub fn metadata(&mut self, values: &[(u8, i32, Value)]) {
        self.start_dirty |= values.iter().any(|v| v.0 == 8);
        self.duration_dirty |= values.iter().any(|v| v.0 == 9);
        self.dirty |= values.iter().any(|v| renders(v.0));
        if self.block.is_some()
            && let Some((_, _, Value::Id(state))) = values.iter().find(|v| v.0 == 23)
        {
            self.block = Some(BlockStateId(*state as u32));
        }
        self.data.apply(values);
    }
    fn tick(&mut self) {
        self.age += 1;
        self.previous = self.position;
        self.old_rotation = self.rotation;
        if self.steps > 0 {
            self.position += (self.target - self.position) / f64::from(self.steps);
            for i in 0..2 {
                self.rotation[i] +=
                    angle(self.target_rotation[i] - self.rotation[i]) / self.steps as f32;
            }
            self.steps -= 1;
        }
        if self.start_dirty {
            self.start = self.age + i64::from(self.data.interpolation_delay);
        }
        if self.duration_dirty {
            self.duration = self.data.interpolation_duration;
        }
        if self.dirty {
            self.from = if self.initialized && self.duration > 0 {
                self.shown.blend(&self.from, self.last_progress)
            } else {
                self.data.clone()
            };
            self.shown = self.data.clone();
            self.initialized = true;
            self.lines = None;
        }
        self.dirty = false;
        self.start_dirty = false;
        self.duration_dirty = false;
    }
    fn move_to(&mut self, position: [f64; 3], rotation: Option<(f32, f32)>) {
        self.target = position.into();
        if let Some((yaw, pitch)) = rotation {
            self.target_rotation = [yaw, pitch];
        }
        self.steps = self.data.teleport_duration.clamp(0, 59);
        if self.steps == 0 {
            self.position = self.target;
            self.previous = self.target;
            self.rotation = self.target_rotation;
            self.old_rotation = self.rotation;
        }
    }
    /// Where a frame draws the display: walked from where it was to where
    /// it heads, or where its carrier stands while it rides one — a move
    /// of its own waits (the carrier carries it).
    fn place(&self, partial: f64, carrier: Option<glam::DVec3>) -> glam::DVec3 {
        carrier.unwrap_or_else(|| self.previous.lerp(self.position, partial))
    }
    /// Whether a frame draws the display at all: a text display with no
    /// text, or an item display whose stack has not come yet.
    fn empty(&self, text_empty: bool) -> bool {
        self.block.is_none() && self.item.is_none() && text_empty
    }
}
fn angle(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

impl Game<'_> {
    pub(super) fn move_remote_text_display(
        &mut self,
        id: i32,
        position: [f64; 3],
        rotation: Option<(f32, f32)>,
    ) {
        if let Some(d) = self.text_displays.get_mut(&id) {
            d.move_to(position, rotation);
        }
    }
    pub(super) fn tick_text_displays(&mut self) {
        for display in self.text_displays.values_mut() {
            display.tick();
        }
    }
    pub(super) fn text_display_geometry(
        &mut self,
        camera: glam::DVec3,
        partial: f64,
        normal: &mut Vec<Vertex>,
        see_through: &mut Vec<Vertex>,
    ) {
        self.text_display_vertices = 0;
        let atlas = &self.gui.atlas;
        let front = self.camera_type == super::perspective::CameraType::ThirdPersonFront;
        let angles = if front {
            [self.yaw + 180.0, -self.pitch]
        } else {
            [self.yaw, self.pitch]
        };
        let scale = self.view_scale();
        // Block displays' models and item displays' stacks, drawn after the
        // loop (it borrows the displays).
        let mut blocks = Vec::new();
        let mut item_work = Vec::new();
        for (id, display) in self.text_displays.iter_mut() {
            display.last_progress = display.progress(partial as f32);
            let d = display.shown.blend(&display.from, display.last_progress);
            // While the display rides a mob (`SetPassengers`), the carrier's
            // interpolated position is its; its own move waits.
            let carrier = display
                .riding
                .and_then(|v| self.mobs.iter().find(|m| m.id == v))
                .map(|m| m.previous.lerp(m.position, partial))
                .map(|p| glam::DVec3::new(p.x, p.y, p.z));
            let position = display.place(partial, carrier);
            let range = f64::from(d.view_range) * 64.0 * scale;
            let empty = display.empty(d.text.is_empty());
            if position.distance_squared(camera) >= range * range || empty {
                continue;
            }
            let rotation = std::array::from_fn(|i| {
                display.old_rotation[i]
                    + angle(display.rotation[i] - display.old_rotation[i]) * partial as f32
            });
            // BlockDisplayRenderer: the block's model, from the display's
            // position as its corner, through the display's transform.
            if let Some(state) = display.block {
                let matrix = glam::Mat4::from_translation((position - camera).as_vec3())
                    * d.matrix(rotation, angles);
                if matrix.is_finite() && state.0 != 0 {
                    blocks.push((state, matrix, position, d.brightness));
                }
                continue;
            }
            // ItemDisplayRenderer: the item's model, resolved with no
            // context (as vanilla's display renders it), through the
            // display's transform; drawn after the loop, which borrows the
            // displays.
            if let Some(stack) = display.item.clone() {
                let matrix = glam::Mat4::from_translation((position - camera).as_vec3())
                    * d.matrix(rotation, angles);
                if matrix.is_finite() {
                    item_work.push((stack, matrix, position, d.brightness, *id));
                }
                continue;
            }
            let mut matrix = glam::Mat4::from_translation((position - camera).as_vec3())
                * d.matrix(rotation, angles)
                * glam::Mat4::from_rotation_y(std::f32::consts::PI)
                * glam::Mat4::from_scale(glam::Vec3::splat(-0.025));
            if !matrix.is_finite() {
                continue;
            }
            // TextDisplayRenderer.splitLines: Font.split, no indent on wrapped lines.
            let Lines { lines, width } = display.lines.get_or_insert_with(|| {
                let lines: Vec<_> = atlas
                    .font
                    .split(&d.text, d.line_width as f32)
                    .into_iter()
                    .map(|line| {
                        let width = atlas.font.width_text(&line).ceil();
                        (line, width)
                    })
                    .collect();
                let width = lines.iter().map(|l| l.1).fold(0.0, f32::max);
                Lines { lines, width }
            });
            let width = *width;
            let height = lines.len() as f32 * 10.0 - 1.0;
            matrix *=
                glam::Mat4::from_translation(glam::Vec3::new(1.0 - width / 2.0, -height, 0.0));
            let mut batch = HudBatch::new([1, 1]);
            let bg = if d.flags & 4 != 0 {
                self.settings.text_background(0.25)
            } else {
                d.background as u32
            };
            if bg != 0 {
                batch.fill(
                    atlas,
                    // submitTextBackground(-1, -1, width, height): corners, not a size.
                    -1.0,
                    -1.0,
                    width + 1.0,
                    height + 1.0,
                    rgba(
                        (bg >> 16) as u8,
                        (bg >> 8) as u8,
                        bg as u8,
                        (bg >> 24) as u8,
                    ),
                );
            }
            let align = d.align();
            for (i, (line, line_width)) in lines.iter().enumerate() {
                let x = match align {
                    Align::Left => 0.0,
                    Align::Right => width - line_width,
                    Align::Center => (width - line_width) / 2.0,
                };
                batch.text_styled(
                    atlas,
                    line,
                    x,
                    i as f32 * 10.0,
                    rgba(255, 255, 255, d.opacity as u8),
                    d.flags & 1 != 0,
                );
            }
            let world_light = self.world.light(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            );
            let light = if d.brightness == -1 {
                horizon_client::block_entity_render::vertex_light(world_light)
            } else {
                ((d.brightness as u32 >> 20 & 15) * 16)
                    | (((d.brightness as u32 >> 4 & 15) * 16) << 8)
            };
            let through = d.flags & 2 != 0;
            let out = if through {
                &mut *see_through
            } else {
                &mut *normal
            };
            for quad in batch.vertices.as_chunks::<6>().0 {
                for i in [0, 1, 2, 5] {
                    let v = quad[i];
                    let point =
                        matrix.transform_point3(glam::Vec3::new(v.pos[0], v.pos[1], v.depth));
                    out.push(Vertex {
                        pos: point.to_array(),
                        uv: v.uv,
                        color: v.color,
                        light,
                        layer: if through { 2 } else { 0 },
                        normal: [0; 4],

                        glint_uv: [0.0, 0.0],
                    });
                }
            }
            self.text_display_vertices += batch.vertices.len() / 6 * 4;
            self.map_patches.extend(batch.patches);
        }
        for (state, matrix, position, brightness) in blocks {
            let light = if brightness == -1 {
                let world = self.world.light(
                    position.x.floor() as i32,
                    position.y.floor() as i32,
                    position.z.floor() as i32,
                );
                (u32::from(world >> 4) * 16) | ((u32::from(world & 15) * 16) << 8)
            } else {
                ((brightness as u32 >> 20 & 15) * 16) | (((brightness as u32 >> 4 & 15) * 16) << 8)
            };
            let start = normal.len();
            self.block_model_vertices(state, 0, matrix, light, true, normal);
            self.text_display_vertices += normal.len() - start;
        }
        for (stack, matrix, position, brightness, id) in item_work {
            let light = if brightness == -1 {
                let world = self.world.light(
                    position.x.floor() as i32,
                    position.y.floor() as i32,
                    position.z.floor() as i32,
                );
                horizon_client::block_entity_render::vertex_light(world)
            } else {
                ((brightness as u32 >> 20 & 15) * 16) | (((brightness as u32 >> 4 & 15) * 16) << 8)
            };
            let start = normal.len();
            let state = self.resolve_item(
                &stack,
                horizon_assets::item_model::DisplayContext::None,
                None,
                id,
            );
            self.submit_item_world(&state, matrix, light, normal);
            self.text_display_vertices += normal.len() - start;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_and_position_interpolation_keep_their_separate_clocks() {
        let mut d = RemoteTextDisplay::new([0.0; 3], 0.0);
        d.tick();
        d.metadata(&[
            (8, 1, Value::Int(0)),
            (9, 1, Value::Int(4)),
            (10, 1, Value::Int(2)),
            (11, 0, Value::Floats(vec![4.0, 0.0, 0.0])),
        ]);
        d.tick();
        assert_eq!(d.shown.blend(&d.from, d.progress(0.5)).translation[0], 0.5);
        d.move_to([2.0, 0.0, 0.0], Some((90.0, 0.0)));
        d.tick();
        assert_eq!(d.position.x, 1.0);
        assert_eq!(d.rotation[0], 45.0);
        d.tick();
        assert_eq!(d.position.x, 2.0);
        assert_eq!(d.shown.blend(&d.from, d.progress(0.0)).translation[0], 2.0);
    }

    /// An item display draws nothing until its data brings the stack, as a
    /// block display shows air: `spawn_remote_object` has no stack to give.
    #[test]
    fn an_item_display_draws_nothing_until_its_stack_comes() {
        let mut d = RemoteTextDisplay::new_item([0.0; 3], 0.0);
        d.tick();
        assert!(d.empty(d.shown.text.is_empty()));
        d.item = Some(horizon_client::items::ItemStack {
            item: "minecraft:nether_star".into(),
            count: 1,
            damage: 0,
            extra: None,
        });
        assert!(!d.empty(d.shown.text.is_empty()));
    }

    /// While a display rides a mob, the carrier's position is the display's;
    /// a move of its own — even one that would snap it there at once —
    /// waits until it is put down.
    #[test]
    fn a_riding_display_sits_where_its_carrier_is() {
        let mut d = RemoteTextDisplay::new([0.0; 3], 0.0);
        d.riding = Some(7);
        d.move_to([10.0, 0.0, 0.0], None);
        d.tick();
        let carrier = glam::DVec3::new(3.0, 64.0, 3.0);
        assert_eq!(d.place(0.5, Some(carrier)), carrier);
        d.riding = None;
        d.move_to([10.0, 0.0, 0.0], None);
        d.tick();
        assert_eq!(d.place(0.5, None).x, 10.0);
    }
}
