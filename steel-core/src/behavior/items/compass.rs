use std::borrow::Cow;
use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::{
    blocks::block_state_ext::BlockStateExt,
    data_components::{
        components::{GlobalPos, LodestoneTracker},
        vanilla_components::LODESTONE_TRACKER,
    },
    item_stack::ItemStack,
    sound_events, vanilla_blocks,
};
use text_components::TextComponent;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::player::Player;
use crate::world::World;

use super::dynamic_name::{default_name, translated};

/// Compass item behavior implementing lodestone binding and validation.
#[item_behavior]
pub struct CompassItem;

impl ItemBehavior for CompassItem {
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        if stack.has(LODESTONE_TRACKER) {
            translated("item.minecraft.lodestone_compass".to_owned(), None)
        } else {
            default_name(stack)
        }
    }

    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;
        let block_state = context.world.get_block_state(pos);

        if block_state.get_block() != &vanilla_blocks::LODESTONE {
            return InteractionResult::Pass;
        }

        context.world.play_block_sound(
            &sound_events::ITEM_LODESTONE_COMPASS_LOCK,
            pos,
            1.0,
            1.0,
            None,
        );

        let dimension = context.world.key.clone();
        let tracker = LodestoneTracker::new(Some(GlobalPos::new(dimension, pos)), true);

        let result_stack = context.inv.with_item(|item| {
            let mut stack = item.clone();
            stack.set_count(1);
            stack.set(LODESTONE_TRACKER, tracker);
            stack
        });

        let leftover = context.inv.with_inventory(|inv| {
            inv.apply_filled_result(
                context.hand,
                result_stack,
                context.player.has_infinite_materials(),
                true,
            )
        });

        if !leftover.is_empty() {
            let _ = context.player.drop_item(leftover, false, true);
        }

        InteractionResult::Success
    }

    fn inventory_tick(
        &self,
        stack: &mut ItemStack,
        world: &Arc<World>,
        _player: &Player,
        _slot: usize,
        _selected: bool,
    ) {
        let Some(tracker) = stack.get(LODESTONE_TRACKER) else {
            return;
        };

        if !tracker.tracked() {
            return;
        }

        let Some(target) = tracker.target() else {
            return;
        };

        if world.key != *target.dimension() {
            return;
        }

        let target_pos = target.pos();
        if !world.is_full_chunk_loaded_at(target_pos) {
            return;
        }

        let block_state = world.get_block_state(target_pos);
        if block_state.get_block() != &vanilla_blocks::LODESTONE {
            let new_tracker = LodestoneTracker::new(None, true);
            stack.set(LODESTONE_TRACKER, new_tracker);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::init_globals_once;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
    use glam::DVec3;
    use steel_registry::blocks::properties::Direction;
    use steel_registry::items::item::BlockHitResult;
    use steel_registry::vanilla_items;
    use steel_utils::types::InteractionHand;
    use steel_utils::types::UpdateFlags;
    use steel_utils::{BlockPos, ChunkPos};
    use uuid::Uuid;

    #[test]
    fn compass_binds_to_lodestone_and_invalidates_when_removed() {
        init_globals_once();

        let world = fresh_test_world("compass_test_world");
        let pos = BlockPos::new(0, 64, 0);
        let chunk_pos = ChunkPos::from_block_pos(pos);
        insert_ready_full_chunk(&world, chunk_pos);

        assert!(world.set_block(
            pos,
            vanilla_blocks::LODESTONE.default_state(),
            UpdateFlags::UPDATE_ALL
        ));

        let player = TestPlayerBuilder::new(Arc::clone(&world), "CompassTester", 1)
            .uuid(Uuid::from_u128(1))
            .build();

        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::new(&vanilla_items::COMPASS));

        let hit_result = BlockHitResult {
            location: DVec3::new(0.5, 64.5, 0.5),
            direction: Direction::Up,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        };
        let mut context = UseOnContext::new(
            &player,
            InteractionHand::MainHand,
            hit_result,
            &world,
            player.inventory.clone(),
        );

        let behavior = CompassItem;

        let result = behavior.use_on(&mut context);
        assert_eq!(result, InteractionResult::Success);

        let mut compass = player.inventory.lock().get_selected_item().clone();
        assert!(!compass.is_empty());
        let tracker = compass
            .get(LODESTONE_TRACKER)
            .expect("should have lodestone tracker component");
        assert!(tracker.tracked());
        let target = tracker.target().expect("should have target global pos");
        assert_eq!(target.pos(), pos);
        assert_eq!(*target.dimension(), world.key);

        behavior.inventory_tick(&mut compass, &world, &player, 0, true);
        let tracker = compass
            .get(LODESTONE_TRACKER)
            .expect("should keep lodestone tracker component");
        assert!(tracker.target().is_some());

        assert!(world.set_block(
            pos,
            vanilla_blocks::AIR.default_state(),
            UpdateFlags::UPDATE_ALL
        ));
        behavior.inventory_tick(&mut compass, &world, &player, 0, true);
        let tracker = compass
            .get(LODESTONE_TRACKER)
            .expect("should keep lodestone tracker component");
        assert!(
            tracker.target().is_none(),
            "target should be invalidated and set to None"
        );
    }
}
