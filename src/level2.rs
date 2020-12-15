/*
    bring stone, mage and random controling from level1 and add

    a level2.map file with . for stones and M for mages.
    on map change the changed entities despawn and respawn.
*/

use bevy::prelude::*;

use bevy::{
    asset::{AssetLoader, LoadContext, LoadedAsset},
    reflect::TypeUuid,
    utils::BoxedFuture,
};
use serde::Deserialize;

use crate::bitpack::Bitpack;

use crate::level1::*;

pub struct Level2Plugin;

impl Plugin for Level2Plugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_startup_system(setup)
            .add_startup_system(add_camera)
            .add_system(kinematic_system)
            .add_system(control_random_movement_system)
            .add_system(control_random_item_basics_system)
            .add_system(carry_system)
            .add_system(throw_system)
            .add_system(sync_tilemap_spawner_system)
            .add_system(tilemap_spawn_handler_system::<EntityFactory>)
            .add_asset::<TileMap>()
            .init_asset_loader::<TileMapLoader>()
            .add_event::<TileMapSpawnEvent>();
    }
}

struct EntityFactory;

impl EntityFactory {
    fn spawn_mage(bundle: TileBundle, commands: &mut Commands, bitpack: &Res<Bitpack>) {
        commands
            .spawn(bundle)
            .with_bundle((
                Mage,
                CanItemBasics {
                    pick_up: true,
                    drop: true,
                    throw: true,
                    picked_up: None,
                },
                Kinematics {
                    vel: Vec3::zero(),
                    drag: 0.97,
                },
                MovementAbility { top_speed: 20.0 },
                ControlRandomMovement {
                    timer: Timer::from_seconds(1.0, true),
                },
                ControlRandomItemBasics {
                    timer: Timer::from_seconds(1.1, true),
                },
            ))
            .with_children(|child| {
                child.spawn(dress_mage(bitpack.atlas_handle.clone()));
            });
    }

    fn spawn_stone(bundle: TileBundle, commands: &mut Commands, bitpack: &Res<Bitpack>) {
        use ContactType::*;
        use SoundType::*;

        commands
            .spawn(bundle)
            .with_bundle((
                Stone,
                CanBeItemBasics {
                    pick_up: true,
                    drop: true,
                    throw: true,
                },
                Kinematics {
                    vel: Vec3::zero(),
                    drag: 0.97,
                },
                SoundOnContact::new(vec![(Ground, Clonk), (Wall, Bling)]),
            ))
            .with_children(|child| {
                child.spawn(dress_stone(bitpack.atlas_handle.clone()));
            });
    }

    fn spawn_sprite(
        bundle: TileBundle,
        commands: &mut Commands,
        bitpack: &Res<Bitpack>,
        index: u32,
        color: Color,
    ) {
        commands
            .spawn(SpriteSheetBundle {
                texture_atlas: bitpack.atlas_handle.clone(),
                sprite: TextureAtlasSprite { index, color },
                ..Default::default()
            })
            .with_bundle(bundle);
    }
}

impl SpawnHandler for EntityFactory {
    type Args = (&'static mut Commands, Res<'static, Bitpack>);

    fn spawn(bundle: TileBundle, (commands, bitpack): &mut Self::Args) {
        println!("MyHandler::spawn {}", bundle.2.translation);

        match bundle.0 .0 as char {
            'M' => Self::spawn_mage(bundle, commands, bitpack),
            '.' => Self::spawn_stone(bundle, commands, bitpack),
            'A' => Self::spawn_sprite(bundle, commands, bitpack, 49, Color::DARK_GREEN),
            'a' => Self::spawn_sprite(bundle, commands, bitpack, 48, Color::DARK_GREEN),
            _ => (),
        }
    }

    fn despawn(a_tile: Entity, (commands, _bitpack): &mut Self::Args) {
        println!("MyHandler::despawn");
        commands.despawn_recursive(a_tile);
    }
}

pub fn setup(commands: &mut Commands, asset_server: Res<AssetServer>) {
    asset_server.watch_for_changes().unwrap();

    let tilemap_handle: Handle<TileMap> = asset_server.load("level2.tilemap");

    let level2_tilemap = (
        Transform::from_translation(Vec3::new(-64.0, 64.0, 0.0)),
        GlobalTransform::default(),
        TileMapSpawner::new(tilemap_handle),
        Children::default(),
    );

    commands.spawn(level2_tilemap);
}

struct TileMapSpawner {
    handle: Handle<TileMap>,
    width: f32,
    height: f32,
}

impl TileMapSpawner {
    fn new(handle: Handle<TileMap>) -> Self {
        Self {
            handle,
            width: 16.0,
            height: 16.0,
        }
    }

    fn spawn(a_spawner: Entity, spawner: &TileMapSpawner, tile: &Tile) -> TileMapSpawnEvent {
        TileMapSpawnEvent::Spawn((
            *tile,
            Parent(a_spawner),
            Transform::from_translation(Vec3::new(
                spawner.width * tile.1 as f32,
                -spawner.height * tile.2 as f32,
                0.0,
            )),
            GlobalTransform::default(),
        ))
    }

    fn despawn(a_tile: Entity) -> TileMapSpawnEvent {
        TileMapSpawnEvent::Despawn(a_tile)
    }
}

enum TileMapSpawnEvent {
    Spawn(TileBundle),
    Despawn(Entity),
}

type TileBundle = (Tile, Parent, Transform, GlobalTransform);

fn sync_tilemap_spawner_system(
    // assets
    tilemaps: Res<Assets<TileMap>>,
    // events
    mut event_reader: Local<EventReader<AssetEvent<TileMap>>>,
    events: Res<Events<AssetEvent<TileMap>>>,
    mut spawn_events: ResMut<Events<TileMapSpawnEvent>>,
    // queries
    spawner_query: Query<(Entity, &TileMapSpawner)>,
    tile_query: Query<(Entity, &Tile, &Parent)>,
) {
    let events = event_reader.iter(&events).map(MyAssetEvent::from);

    for MyAssetEvent(event, handle) in events {
        match event {
            Event::Created => {
                let tilemap = tilemaps.get(handle.clone()).unwrap();

                for (a_spawner, spawner) in spawner_query.iter().filter(|(_, t)| t.handle == handle)
                {
                    for tile in tilemap.map.iter() {
                        spawn_events.send(TileMapSpawner::spawn(a_spawner, spawner, tile));
                    }
                }
            }
            Event::Modified => {
                let tilemap = tilemaps.get(handle.clone()).unwrap();

                for (a_spawner, spawner) in spawner_query.iter().filter(|(_, t)| t.handle == handle)
                {
                    let relevant = tile_query
                        .iter()
                        .filter(|(_, _, Parent(parent))| parent == &a_spawner)
                        .collect::<Vec<_>>();
                    let existing_tiles: Vec<_> =
                        relevant.iter().map(|(_, &tile, _)| tile).collect();

                    for (a_tile, tile, _) in relevant.iter() {
                        if !tilemap.map.contains(tile) {
                            spawn_events.send(TileMapSpawner::despawn(*a_tile));
                        }
                    }

                    for tile in tilemap.map.iter() {
                        if !existing_tiles.contains(tile) {
                            spawn_events.send(TileMapSpawner::spawn(a_spawner, spawner, tile));
                        }
                    }
                }
            }
            Event::Removed => todo!(),
        }
    }
}

trait SpawnHandler {
    type Args;

    fn spawn(bundle: TileBundle, args: &mut Self::Args);
    fn despawn(a_tile: Entity, args: &mut Self::Args);
}

fn tilemap_spawn_handler_system<T: SpawnHandler>(
    mut args: T::Args,
    mut event_reader: Local<EventReader<TileMapSpawnEvent>>,
    events: Res<Events<TileMapSpawnEvent>>,
) {
    for event in event_reader.iter(&events) {
        match event {
            TileMapSpawnEvent::Spawn(bundle) => T::spawn(*bundle, &mut args),
            TileMapSpawnEvent::Despawn(a_tile) => T::despawn(*a_tile, &mut args),
        };
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Deserialize, Default, Clone, Copy)]
struct Tile(u8, u32, u32);

enum Event {
    Created,
    Modified,
    Removed,
}

struct MyAssetEvent(Event, Handle<TileMap>);

impl From<&AssetEvent<TileMap>> for MyAssetEvent {
    fn from(event: &AssetEvent<TileMap>) -> Self {
        match event {
            AssetEvent::Created { handle } => MyAssetEvent(Event::Created, handle.clone()),
            AssetEvent::Modified { handle } => MyAssetEvent(Event::Modified, handle.clone()),
            AssetEvent::Removed { handle } => MyAssetEvent(Event::Removed, handle.clone()),
        }
    }
}

#[derive(Debug, Deserialize, TypeUuid, Default)]
#[uuid = "39cadc56-aa9c-4543-8640-a018b71b5051"]
struct TileMap {
    map: Vec<Tile>,
}

#[derive(Default)]
pub struct TileMapLoader;

impl AssetLoader for TileMapLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<(), anyhow::Error>> {
        Box::pin(async move {
            println!("TileMapLoader reload");
            let mut map = vec![];
            let mut col = 0;
            let mut row = 0;

            for &c in bytes.iter() {
                if c == '\n' as u8 {
                    col = 0;
                    row += 1;
                } else {
                    if c != ' ' as u8 {
                        map.push(Tile(c, col, row));
                    }
                    col += 1;
                }
            }

            load_context.set_default_asset(LoadedAsset::new(TileMap { map }));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["tilemap"]
    }
}
