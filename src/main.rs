use anyhow::Context as _;
use piccolo::{Callback, CallbackReturn, Closure, Executor, Lua, MetaMethod, Table, UserData, lua};
use piccolo_util::freeze::{Freeze, Frozen};

pub struct World {
    i: i64,
}

impl World {
    fn say_hi(&mut self) {
        println!("say_hi called on world with i={}", self.i);
        self.i += 1;
    }
}

type FrozenWorldInner = Frozen<Freeze![&'freeze mut World]>;

#[derive(Clone, gc_arena::Collect)]
#[collect(require_static)]
pub struct FrozenWorld(FrozenWorldInner);

impl FrozenWorld {
    pub fn new() -> FrozenWorld {
        Self(FrozenWorldInner::new())
    }

    pub fn from_ud<'gc>(ud: UserData<'gc>) -> anyhow::Result<&'gc Self> {
        ud.downcast_static::<FrozenWorld>()
            .context("expected `FrozenWorld (userdata)`, got `Unknown (userdata)`")
    }
}

impl core::ops::Deref for FrozenWorld {
    type Target = FrozenWorldInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn create_global(
    ctx: lua::Context<'_>,
    frozen_world: FrozenWorld,
) -> anyhow::Result<UserData<'_>> {
    let index = Table::new(&ctx);

    index.set(
        ctx,
        "say_hi",
        Callback::from_fn_with(
            &ctx,
            frozen_world.clone(),
            |frozen_world, _ctx, _ex, _stack| {
                frozen_world.with_mut(|world| {
                    world.say_hi();
                });

                Ok(CallbackReturn::Return)
            },
        ),
    )?;

    let world = UserData::new_static(&ctx, frozen_world);
    let meta = Table::new(&ctx);
    meta.set(ctx, MetaMethod::Index, index)?;
    world.set_metatable(&ctx, Some(meta));

    Ok(world)
}

fn run_script_using_world_state(lua: &mut Lua, source: &str, world: &mut World) {
    Frozen::<Freeze![&'freeze mut World]>::in_scope(world, |world| {
        lua.try_enter(|ctx| {
            let frozen_world = FrozenWorld(world);
            let userdata = create_global(ctx, frozen_world.clone())?;
            ctx.globals().set(ctx, "world", userdata)?;
            Ok(())
        })
        .unwrap();

        let ex = lua
            .try_enter(|ctx| {
                let env = ctx.globals();
                let closure = Closure::load_with_env(ctx, None, source.as_bytes(), env)?;

                let ex = Executor::start(ctx, closure.into(), ());
                Ok(ctx.stash(ex))
            })
            .unwrap();

        lua.execute::<()>(&ex).unwrap();
    });
}

fn main() {
    let mut lua = Lua::full();

    let mut world = World { i: 0 };
    let source = include_str!("script.lua");

    run_script_using_world_state(&mut lua, source, &mut world);
    run_script_using_world_state(&mut lua, source, &mut world);
}
