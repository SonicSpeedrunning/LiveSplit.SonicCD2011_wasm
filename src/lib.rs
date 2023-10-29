#![no_std]
#![feature(type_alias_impl_trait, const_async_blocks)]
#![warn(
    clippy::complexity,
    clippy::correctness,
    clippy::perf,
    clippy::style,
    clippy::undocumented_unsafe_blocks,
    rust_2018_idioms
)]

use asr::{
    file_format::pe,
    future::{next_tick, retry},
    settings::Gui,
    signature::Signature,
    time::Duration,
    timer::{self, TimerState},
    watcher::Watcher,
    Address, Address32, Process,
};

asr::panic_handler!();
asr::async_main!(nightly);

async fn main() {
    let mut settings = Settings::register();

    loop {
        // Hook to the target process
        let process = retry(|| PROCESS_NAMES.into_iter().find_map(Process::attach)).await;

        process
            .until_closes(async {
                // Once the target has been found and attached to, set up some default watchers
                let mut watchers = Watchers::default();

                // Perform memory scanning to look for the addresses we need
                let addresses = retry(|| Addresses::init(&process)).await;

                loop {
                    // Splitting logic. Adapted from OG LiveSplit:
                    // Order of execution
                    // 1. update() will always be run first. There are no conditions on the execution of this action.
                    // 2. If the timer is currently either running or paused, then the isLoading, gameTime, and reset actions will be run.
                    // 3. If reset does not return true, then the split action will be run.
                    // 4. If the timer is currently not running (and not paused), then the start action will be run.
                    settings.update();
                    update_loop(&process, &addresses, &mut watchers);

                    let timer_state = timer::state();
                    if timer_state == TimerState::Running || timer_state == TimerState::Paused {
                        if let Some(is_loading) = is_loading(&watchers, &settings) {
                            if is_loading {
                                timer::pause_game_time()
                            } else {
                                timer::resume_game_time()
                            }
                        }

                        if let Some(game_time) = game_time(&watchers, &settings, &addresses) {
                            timer::set_game_time(game_time)
                        }

                        if reset(&watchers, &settings) {
                            timer::reset()
                        } else if split(&watchers, &settings) {
                            timer::split()
                        }
                    }

                    if timer::state() == TimerState::NotRunning && start(&watchers, &settings) {
                        timer::start();
                        timer::pause_game_time();

                        if let Some(is_loading) = is_loading(&watchers, &settings) {
                            if is_loading {
                                timer::pause_game_time()
                            } else {
                                timer::resume_game_time()
                            }
                        }
                    }

                    next_tick().await;
                }
            })
            .await;
    }
}

#[derive(Gui)]
struct Settings {
    #[default = true]
    /// START --> Enable auto start
    start: bool,
    #[default = true]
    /// RESET --> Enable auto reset
    reset: bool,
    #[default = false]
    /// TIMING --> Use All Time Stones timing rules (RTA-TB)
    rta_tb: bool,
    #[default = true]
    /// Palmtree Panic - Act 1
    palmtree_panic_1: bool,
    #[default = true]
    /// Palmtree Panic - Act 2
    palmtree_panic_2: bool,
    #[default = true]
    /// Palmtree Panic - Act 3
    palmtree_panic_3: bool,
    #[default = true]
    /// Collision Chaos - Act 1
    collision_chaos_1: bool,
    #[default = true]
    /// Collision Chaos - Act 2
    collision_chaos_2: bool,
    #[default = true]
    /// Collision Chaos - Act 3
    collision_chaos_3: bool,
    #[default = true]
    /// Tidal Tempest - Act 1
    tidal_tempest_1: bool,
    #[default = true]
    /// Tidal Tempest - Act 2
    tidal_tempest_2: bool,
    #[default = true]
    /// Tidal Tempest - Act 3
    tidal_tempest_3: bool,
    #[default = true]
    /// Quartz Quadrant - Act 1
    quartz_quadrant_1: bool,
    #[default = true]
    /// Quartz Quadrant - Act 2
    quartz_quadrant_2: bool,
    #[default = true]
    /// Quartz Quadrant - Act 3
    quartz_quadrant_3: bool,
    #[default = true]
    /// Wacky Workbench - Act 1
    wacky_workbench_1: bool,
    #[default = true]
    /// Wacky Workbench - Act 2
    wacky_workbench_2: bool,
    #[default = true]
    /// Wacky Workbench - Act 3
    wacky_workbench_3: bool,
    #[default = true]
    /// Stardust Speedway - Act 1
    stardust_speedway_1: bool,
    #[default = true]
    /// Stardust Speedway - Act 2
    stardust_speedway_2: bool,
    #[default = true]
    /// Stardust Speedway - Act 3
    stardust_speedway_3: bool,
    #[default = true]
    /// Metallic Madness - Act 1
    metallic_madness_1: bool,
    #[default = true]
    /// Metallic Madness - Act 2
    metallic_madness_2: bool,
    #[default = true]
    /// Metallic Madness - Act 3
    metallic_madness_3: bool,
}

#[derive(Default)]
struct Watchers {
    accumulated_igt: Duration,
    buffer_igt: Duration,
    igt_offset: Duration,
    time_bonus_start_value: u32,
    demo_mode: Watcher<bool>,
    state: Watcher<u8>,
    time_bonus: Watcher<u32>,
    final_boss_health: Watcher<u8>,
    level_id: Watcher<Acts>,
    timer_is_running: Watcher<bool>,
    igt: Watcher<Duration>,
    centisecs: Watcher<Duration>,
    livesplit_timer_state: Watcher<TimerState>,
}

struct Addresses {
    demo_mode: Address,
    state: Address,
    score_tally_state: Address,
    time_bonus: Address,
    bhp_good: Address,
    bhp_bad: Address,
    level_id: Address,
    level_id_type: Address,
    timer_is_running: Address,
    seconds: Address,
    minutes: Address,
    centisecs: Address,
    has_centisecs_bug: bool,
}

impl Addresses {
    fn init(game: &Process) -> Option<Self> {
        let main_module_base = PROCESS_NAMES
            .into_iter()
            .find_map(|p| game.get_module_address(p).ok())?;
        let main_module_size = pe::read_size_of_image(game, main_module_base)? as u64;

        // Determine game version through signature scanning
        let is_64_bit: bool;
        let game_version: GameVersion;
        let has_centisecs_bug: bool;

        if SIG32_RETAIL
            .scan_process_range(game, (main_module_base, main_module_size))
            .is_some()
        {
            is_64_bit = false;
            game_version = GameVersion::Retail;
            has_centisecs_bug = true;
        } else if SIG32_DECOMP_1_0_0
            .scan_process_range(game, (main_module_base, main_module_size))
            .is_some()
        {
            is_64_bit = false;
            game_version = GameVersion::Decompilation32bit1_0_0;
            has_centisecs_bug = SIG32_DECOMP_TIMERBUG
                .scan_process_range(game, (main_module_base, main_module_size))
                .is_none();
        } else if SIG32_DECOMP_1_3_1
            .scan_process_range(game, (main_module_base, main_module_size))
            .is_some()
        {
            is_64_bit = false;
            game_version = GameVersion::Decompilation32bit1_3_1;
            has_centisecs_bug = false;
        } else if SIG64_DECOMP_1_0_0
            .scan_process_range(game, (main_module_base, main_module_size))
            .is_some()
        {
            is_64_bit = true;
            game_version = GameVersion::Decompilation64bit1_0_0;
            has_centisecs_bug = SIG64_DECOMP_TIMERBUG
                .scan_process_range(game, (main_module_base, main_module_size))
                .is_none();
        } else if SIG64_DECOMP_1_3_1
            .scan_process_range(game, (main_module_base, main_module_size))
            .is_some()
        {
            is_64_bit = true;
            game_version = GameVersion::Decompilation64bit1_3_1;
            has_centisecs_bug = false;
        } else {
            return None;
        }

        // Find addresses
        let ptr: Address;
        let mut lea = Address::NULL;

        let demo_mode: Address;
        let score_tally_state: Address;
        let state: Address;
        let time_bonus: Address;
        let bhp_good: Address;
        let bhp_bad: Address;
        let level_id: Address;
        let level_id_type: Address;
        let timer_is_running: Address;
        let seconds: Address;
        let minutes: Address;
        let centisecs: Address;

        match game_version {
            GameVersion::Retail => {
                ptr = game
                    .read::<Address32>(
                        SIG32_RETAIL
                            .scan_process_range(game, (main_module_base, main_module_size))?
                            + 3,
                    )
                    .ok()?
                    .into();
            }
            GameVersion::Decompilation32bit1_0_0 => {
                ptr = game
                    .read::<Address32>(
                        SIG32_DECOMP_1_0_0
                            .scan_process_range(game, (main_module_base, main_module_size))?
                            + 3,
                    )
                    .ok()?
                    .into();
            }
            GameVersion::Decompilation32bit1_3_1 => {
                ptr = game
                    .read::<Address32>(
                        SIG32_DECOMP_1_3_1
                            .scan_process_range(game, (main_module_base, main_module_size))?
                            + 3,
                    )
                    .ok()?
                    .into();
            }
            GameVersion::Decompilation64bit1_0_0 => {
                let addr = SIG64_DECOMP_1_0_0
                    .scan_process_range(game, (main_module_base, main_module_size))?
                    + 4;
                ptr = main_module_base + game.read::<u32>(addr).ok()?;

                let addr = SIG64_DECOMP_1_0_0_LEA
                    .scan_process_range(game, (main_module_base, main_module_size))?
                    + 3;
                lea = addr + 0x4 + game.read::<u32>(addr).ok()?;
            }
            GameVersion::Decompilation64bit1_3_1 => {
                let addr = SIG64_DECOMP_1_3_1
                    .scan_process_range(game, (main_module_base, main_module_size))?
                    + 4;
                ptr = main_module_base + game.read::<u32>(addr).ok()?;

                let addr = SIG64_DECOMP_1_0_0_LEA
                    .scan_process_range(game, (main_module_base, main_module_size))?
                    + 3;
                lea = addr + 0x4 + game.read::<u32>(addr).ok()?;
            }
        }

        // Scanning function
        let pointerpath = |offset1: u32, offset2: u32, offset3: u32, absolute: bool| -> Address {
            if is_64_bit {
                if offset1 == 0 {
                    return lea + offset3;
                }
                let temp_offset = game.read::<u32>(ptr + offset1).ok().unwrap_or_default();
                let temp_offset2 = main_module_base + temp_offset + offset2;
                if absolute {
                    main_module_base
                        + game.read::<u32>(temp_offset2).ok().unwrap_or_default()
                        + offset3
                } else {
                    temp_offset2
                        + 0x4
                        + game.read::<u32>(temp_offset2).ok().unwrap_or_default()
                        + offset3
                }
            } else {
                (game
                    .read_pointer_path32::<Address32>(ptr + offset1, &[0, offset2])
                    .ok()
                    .unwrap_or_default()
                    + offset3)
                    .into()
            }
        };

        match game_version {
            GameVersion::Retail => {
                demo_mode = pointerpath(0x4 * 11, 16, 0x1AC, true);
                level_id_type = pointerpath(0x4 * 119, 12, 0, true);
                level_id = pointerpath(0x4 * 120, 12, 0, true);
                timer_is_running = pointerpath(0x4 * 121, 11, 0, true);
                state = pointerpath(0x4 * 19, 18, 0x1078, true);
                score_tally_state = pointerpath(0x4 * 19, 18, 0x7F8, true);
                time_bonus = pointerpath(0x4 * 37, 18, 0x7F8, true);
                bhp_good = pointerpath(0x4 * 32, 18, 0x37C8, true);
                bhp_bad = pointerpath(0x4 * 32, 18, 0x380C, true);

                let ptr = SIG32_RETAIL_CENTISECS
                    .scan_process_range(game, (main_module_base, main_module_size))?;
                centisecs = game.read::<Address32>(ptr + 1).ok()?.into();
                seconds = game.read::<Address32>(ptr + 35).ok()?.into();
                minutes = game.read::<Address32>(ptr + 69).ok()?.into();
            }
            GameVersion::Decompilation32bit1_0_0 => {
                demo_mode = pointerpath(0x4 * 11, 10, 0x1AC, true);
                level_id_type = pointerpath(0x4 * 119, 8, 0, true);
                level_id = pointerpath(0x4 * 120, 8, 0, true);
                timer_is_running = pointerpath(0x4 * 121, 11, 0, true);
                state = pointerpath(0x4 * 19, 17, 0x1078, true);
                score_tally_state = pointerpath(0x4 * 19, 17, 0x7F8, true);
                time_bonus = pointerpath(0x4 * 37, 17, 0x7F8, true);
                bhp_good = pointerpath(0x4 * 32, 17, 0x37C8, true);
                bhp_bad = pointerpath(0x4 * 32, 17, 0x380C, true);

                let ptr = SIG32_DECOMP_CENTISECS
                    .scan_process_range(game, (main_module_base, main_module_size))?;
                centisecs = game.read::<Address32>(ptr + 2).ok()?.into();
                seconds = game.read::<Address32>(ptr + 29).ok()?.into();
                minutes = game.read::<Address32>(ptr + 51).ok()?.into();
            }
            GameVersion::Decompilation32bit1_3_1 => {
                demo_mode = pointerpath(0x4 * 11, 10, 0x1AC, true);
                level_id_type = pointerpath(0x4 * 119, 9, 0, true);
                level_id = pointerpath(0x4 * 120, 9, 0, true);
                timer_is_running = pointerpath(0x4 * 121, 11, 0, true);
                state = pointerpath(0x4 * 19, 17, 0x1078, true);
                score_tally_state = pointerpath(0x4 * 19, 17, 0x7F8, true);
                time_bonus = pointerpath(0x4 * 37, 17, 0x7F8, true);
                bhp_good = pointerpath(0x4 * 32, 17, 0x37C8, true);
                bhp_bad = pointerpath(0x4 * 32, 17, 0x380C, true);

                let ptr = SIG32_DECOMP_CENTISECS
                    .scan_process_range(game, (main_module_base, main_module_size))?;
                centisecs = game.read::<Address32>(ptr + 2).ok()?.into();
                seconds = game.read::<Address32>(ptr + 29).ok()?.into();
                minutes = game.read::<Address32>(ptr + 51).ok()?.into();
            }
            GameVersion::Decompilation64bit1_0_0 | GameVersion::Decompilation64bit1_3_1 => {
                demo_mode = pointerpath(0x4 * 11, 15, 0x1AC, true);
                level_id_type = pointerpath(0x4 * 119, 10, 0, false);
                level_id = pointerpath(0x4 * 120, 10, 0, false);
                timer_is_running = pointerpath(0x4 * 121, 12, 0, false);
                state = pointerpath(0, 0, 0x10B2, false);
                score_tally_state = pointerpath(0, 0, 0x832, false);
                time_bonus = pointerpath(0, 0, 0x814, false);
                bhp_good = pointerpath(0, 0, 0x37D0, false);
                bhp_bad = pointerpath(0, 0, 0x3814, false);

                if let Some(ptr) = SIG64_DECOMP_CENTISECS
                    .scan_process_range(game, (main_module_base, main_module_size))
                {
                    let mut addr = ptr + 2;
                    centisecs = addr + 0x4 + game.read::<u32>(addr).ok()?;
                    addr = ptr + 29;
                    seconds = addr + 0x4 + game.read::<u32>(addr).ok()?;
                    addr = ptr + 54;
                    minutes = addr + 0x4 + game.read::<u32>(addr).ok()?;
                } else {
                    let ptr = SIG64_DECOMP_CENTISECS_ALT
                        .scan_process_range(game, (main_module_base, main_module_size))?;
                    let mut addr = ptr + 2;
                    centisecs = addr + 0x4 + game.read::<u32>(addr).ok()? as u64;
                    addr = ptr + 31;
                    seconds = addr + 0x4 + game.read::<u32>(addr).ok()? as u64;
                    addr = ptr + 57;
                    minutes = addr + 0x4 + game.read::<u32>(addr).ok()? as u64;
                }
            }
        };

        Some(Self {
            demo_mode,
            state,
            score_tally_state,
            time_bonus,
            bhp_good,
            bhp_bad,
            level_id,
            level_id_type,
            timer_is_running,
            seconds,
            minutes,
            centisecs,
            has_centisecs_bug,
        })
    }
}

fn update_loop(game: &Process, addresses: &Addresses, watchers: &mut Watchers) {
    // LiveSplit's timer state, defined inside a watcher in order to define some actions when the timer starts or resets
    let Some(timer_state) = watchers.livesplit_timer_state.update(Some(timer::state())) else {
        return;
    };

    // Update standard values
    watchers.demo_mode.update(Some(
        game.read::<u8>(addresses.demo_mode)
            .ok()
            .unwrap_or_default()
            > 0,
    ));
    watchers.state.update(game.read(addresses.state).ok());
    watchers.timer_is_running.update(Some(
        game.read::<u8>(addresses.timer_is_running)
            .ok()
            .unwrap_or_default()
            > 0,
    ));

    // Level ID
    match game
        .read::<u8>(addresses.score_tally_state)
        .ok()
        .unwrap_or_default()
    {
        0 => {
            let lid = game
                .read::<u8>(addresses.level_id_type)
                .ok()
                .unwrap_or_default() as u32
                * 100
                + game.read::<u8>(addresses.level_id).ok().unwrap_or_default() as u32;
            let current_act = match lid {
                0 => Acts::TitleScreen,
                1 => Acts::MainMenu,
                2 => Acts::TimeAttack,
                8 => Acts::Credits,
                100..=103 => Acts::PalmtreePanicAct1,
                104..=107 => Acts::PalmtreePanicAct2,
                108 | 109 => Acts::PalmtreePanicAct3,
                110..=113 => Acts::CollisionChaosAct1,
                114..=117 => Acts::CollisionChaosAct2,
                118 | 119 => Acts::CollisionChaosAct3,
                120..=123 => Acts::TidalTempestAct1,
                124..=127 => Acts::TidalTempestAct2,
                128 | 129 => Acts::TidalTempestAct3,
                130..=133 => Acts::QuartzQuadrantAct1,
                134..=137 => Acts::QuartzQuadrantAct2,
                138 | 139 => Acts::QuartzQuadrantAct3,
                140..=143 => Acts::WackyWorkbenchAct1,
                144..=147 => Acts::WackyWorkbenchAct2,
                148 | 149 => Acts::WackyWorkbenchAct3,
                150..=153 => Acts::StardustSpeedwayAct1,
                154..=157 => Acts::StardustSpeedwayAct2,
                158 | 159 => Acts::StardustSpeedwayAct3,
                160..=163 => Acts::MetallicMadnessAct1,
                164..=167 => Acts::MetallicMadnessAct2,
                168 | 169 => Acts::MetallicMadnessAct3,
                _ => match &watchers.level_id.pair {
                    Some(x) => x.current,
                    _ => Acts::PalmtreePanicAct1,
                },
            };
            watchers.level_id.update(Some(current_act));

            let final_boss_health = match lid {
                168 => game.read::<u8>(addresses.bhp_good).ok().unwrap_or_default(),
                169 => game.read::<u8>(addresses.bhp_bad).ok().unwrap_or_default(),
                _ => 0xFF,
            };
            watchers.final_boss_health.update(Some(final_boss_health));
        }
        _ => {
            watchers
                .level_id
                .update(Some(match &watchers.level_id.pair {
                    Some(x) => x.current,
                    _ => Acts::PalmtreePanicAct1,
                }));
            watchers.final_boss_health.update(Some(0xFF));
        }
    };

    // IGT logic
    let Some(demo_mode) = &watchers.demo_mode.pair else {
        return;
    };
    let Some(timer_is_running) = &watchers.timer_is_running.pair else {
        return;
    };

    let centisecs = (game
        .read::<u8>(addresses.centisecs)
        .ok()
        .unwrap_or_default() as u64
        * 100)
        / 60;
    let Some(centis) = watchers
        .centisecs
        .update(Some(Duration::milliseconds(centisecs as i64 * 10)))
    else {
        return;
    };

    let new_igt =
        if demo_mode.current || demo_mode.old || timer_state.current == TimerState::NotRunning {
            Duration::ZERO
        } else if !timer_is_running.old && !timer_is_running.current {
            match &watchers.igt.pair {
                Some(x) => x.current,
                _ => Duration::ZERO,
            }
        } else {
            let mins = game.read::<u8>(addresses.minutes).ok().unwrap_or_default() as u64;
            let secs = game.read::<u8>(addresses.seconds).ok().unwrap_or_default() as u64;
            Duration::milliseconds(
                (mins * 60000
                    + secs * 1000
                    + if addresses.has_centisecs_bug {
                        0
                    } else {
                        centisecs
                    } * 10) as i64,
            )
        };
    let Some(final_igt) = watchers.igt.update(Some(new_igt)) else {
        return;
    };

    // Reset the buffer IGT variables when the timer is stopped
    if timer_state.current == TimerState::NotRunning {
        watchers.accumulated_igt = Duration::ZERO;
        watchers.buffer_igt = Duration::ZERO;
        watchers.igt_offset = Duration::ZERO;
    }

    if final_igt.old > final_igt.current {
        watchers.accumulated_igt += final_igt.old - watchers.buffer_igt;
        watchers.buffer_igt = final_igt.current;
    }

    // Set the IGT offset when starting a new run, if the game has the centisecs bug
    if addresses.has_centisecs_bug
        && timer_state.old == TimerState::NotRunning
        && timer_state.current == TimerState::Running
    {
        watchers.igt_offset = centis.current;
    }

    // Time bonus start value
    let Some(time_bonus) = watchers
        .time_bonus
        .update(game.read::<u32>(addresses.time_bonus).ok())
    else {
        return;
    };
    if time_bonus.old == 0 && time_bonus.changed() {
        watchers.time_bonus_start_value = time_bonus.current
    } else if time_bonus.current == 0 {
        watchers.time_bonus_start_value = 0
    }
}

fn start(watchers: &Watchers, settings: &Settings) -> bool {
    if !settings.start {
        return false;
    }
    let Some(act) = &watchers.level_id.pair else {
        return false;
    };
    let Some(state) = &watchers.state.pair else {
        return false;
    };
    act.current == Acts::MainMenu && state.current == 7 && state.old == 6
}

fn split(watchers: &Watchers, settings: &Settings) -> bool {
    let Some(act) = &watchers.level_id.pair else {
        return false;
    };

    match act.old {
        Acts::PalmtreePanicAct1 => {
            settings.palmtree_panic_1 && act.current == Acts::PalmtreePanicAct2
        }
        Acts::PalmtreePanicAct2 => {
            settings.palmtree_panic_2 && act.current == Acts::PalmtreePanicAct3
        }
        Acts::PalmtreePanicAct3 => {
            settings.palmtree_panic_3 && act.current == Acts::CollisionChaosAct1
        }
        Acts::CollisionChaosAct1 => {
            settings.collision_chaos_1 && act.current == Acts::CollisionChaosAct2
        }
        Acts::CollisionChaosAct2 => {
            settings.collision_chaos_2 && act.current == Acts::CollisionChaosAct3
        }
        Acts::CollisionChaosAct3 => {
            settings.collision_chaos_3 && act.current == Acts::TidalTempestAct1
        }
        Acts::TidalTempestAct1 => settings.tidal_tempest_1 && act.current == Acts::TidalTempestAct2,
        Acts::TidalTempestAct2 => settings.tidal_tempest_2 && act.current == Acts::TidalTempestAct3,
        Acts::TidalTempestAct3 => {
            settings.tidal_tempest_3 && act.current == Acts::QuartzQuadrantAct1
        }
        Acts::QuartzQuadrantAct1 => {
            settings.quartz_quadrant_1 && act.current == Acts::QuartzQuadrantAct2
        }
        Acts::QuartzQuadrantAct2 => {
            settings.quartz_quadrant_2 && act.current == Acts::QuartzQuadrantAct3
        }
        Acts::QuartzQuadrantAct3 => {
            settings.quartz_quadrant_3 && act.current == Acts::WackyWorkbenchAct1
        }
        Acts::WackyWorkbenchAct1 => {
            settings.wacky_workbench_1 && act.current == Acts::WackyWorkbenchAct2
        }
        Acts::WackyWorkbenchAct2 => {
            settings.wacky_workbench_2 && act.current == Acts::WackyWorkbenchAct3
        }
        Acts::WackyWorkbenchAct3 => {
            settings.wacky_workbench_3 && act.current == Acts::StardustSpeedwayAct1
        }
        Acts::StardustSpeedwayAct1 => {
            settings.stardust_speedway_1 && act.current == Acts::StardustSpeedwayAct2
        }
        Acts::StardustSpeedwayAct2 => {
            settings.stardust_speedway_2 && act.current == Acts::StardustSpeedwayAct3
        }
        Acts::StardustSpeedwayAct3 => {
            settings.stardust_speedway_3 && act.current == Acts::MetallicMadnessAct1
        }
        Acts::MetallicMadnessAct1 => {
            settings.metallic_madness_1 && act.current == Acts::MetallicMadnessAct2
        }
        Acts::MetallicMadnessAct2 => {
            settings.metallic_madness_2 && act.current == Acts::MetallicMadnessAct3
        }
        Acts::MetallicMadnessAct3 => {
            settings.metallic_madness_3 && {
                let Some(finalboss_hp) = &watchers.final_boss_health.pair else {
                    return false;
                };
                let Some(igt) = &watchers.igt.pair else {
                    return false;
                };
                if settings.rta_tb {
                    (act.current == Acts::Credits || act.current == Acts::MainMenu)
                        && finalboss_hp.old == 0
                        && igt.old != Duration::ZERO
                } else {
                    finalboss_hp.old == 1
                        && finalboss_hp.current == 0
                        && igt.current != Duration::ZERO
                }
            }
        }
        _ => false,
    }
}

fn reset(watchers: &Watchers, settings: &Settings) -> bool {
    if !settings.reset {
        return false;
    }
    let Some(act) = &watchers.level_id.pair else {
        return false;
    };
    let Some(state) = &watchers.state.pair else {
        return false;
    };
    act.current == Acts::MainMenu && state.current == 5 && state.changed()
}

fn is_loading(watchers: &Watchers, settings: &Settings) -> Option<bool> {
    if settings.rta_tb {
        let Some(time_bonus) = &watchers.time_bonus.pair else {
            return None;
        };
        Some(
            watchers.time_bonus_start_value != 0
                && time_bonus.current != watchers.time_bonus_start_value,
        )
    } else {
        Some(true)
    }
}

fn game_time(watchers: &Watchers, settings: &Settings, addresses: &Addresses) -> Option<Duration> {
    if settings.rta_tb {
        None
    } else {
        let Some(igt) = &watchers.igt.pair else {
            return None;
        };
        let Some(centisecs) = &watchers.centisecs.pair else {
            return None;
        };
        Some(
            igt.current + watchers.accumulated_igt - watchers.buffer_igt - watchers.igt_offset
                + if addresses.has_centisecs_bug {
                    centisecs.current
                } else {
                    Duration::ZERO
                },
        )
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Acts {
    TitleScreen,
    MainMenu,
    TimeAttack,
    PalmtreePanicAct1,
    PalmtreePanicAct2,
    PalmtreePanicAct3,
    CollisionChaosAct1,
    CollisionChaosAct2,
    CollisionChaosAct3,
    TidalTempestAct1,
    TidalTempestAct2,
    TidalTempestAct3,
    QuartzQuadrantAct1,
    QuartzQuadrantAct2,
    QuartzQuadrantAct3,
    WackyWorkbenchAct1,
    WackyWorkbenchAct2,
    WackyWorkbenchAct3,
    StardustSpeedwayAct1,
    StardustSpeedwayAct2,
    StardustSpeedwayAct3,
    MetallicMadnessAct1,
    MetallicMadnessAct2,
    MetallicMadnessAct3,
    Credits,
}

#[derive(Clone, Copy, PartialEq)]
enum GameVersion {
    Retail,
    Decompilation32bit1_0_0, // Valid from base version up tp v1.3.0)
    Decompilation32bit1_3_1, // Valid from v1.3.1 onwards
    Decompilation64bit1_0_0,
    Decompilation64bit1_3_1,
}

const PROCESS_NAMES: [&str; 9] = [
    "soniccd.exe",
    "RSDKv3.exe",
    "RSDKv3_64.exe",
    "RSDKv3_HW.exe",
    "RSDKv3_HW_64.exe",
    "Sonic CD.exe",
    "Sonic CD_64.exe",
    "Restored.exe",
    "Legacy.exe",
];

const SIG32_RETAIL: Signature<13> = Signature::new("FF 24 85 ?? ?? ?? ?? 8B 4D F0 8B 14 8D");
const SIG32_RETAIL_CENTISECS: Signature<15> =
    Signature::new("A2 ?? ?? ?? ?? 0F B6 0D ?? ?? ?? ?? 83 F9 3C");

const SIG32_DECOMP_1_0_0: Signature<10> = Signature::new("FF 24 85 ?? ?? ?? ?? 8B 04 B5");
const SIG32_DECOMP_1_3_1: Signature<10> = Signature::new("FF 24 8D ?? ?? ?? ?? 8B 0C 85");
const SIG32_DECOMP_CENTISECS: Signature<8> = Signature::new("89 0D ?? ?? ?? ?? 3B CE");
const SIG32_DECOMP_TIMERBUG: Signature<34> = Signature::new("C6 05 ?? ?? ?? ?? 00 C6 05 ?? ?? ?? ?? 00 C7 05 ?? ?? ?? ?? 00 00 00 00 C7 05 ?? ?? ?? ?? 00 00 00 00");

const SIG64_DECOMP_1_0_0: Signature<11> = Signature::new("41 8B 8C 8C ?? ?? ?? ?? 49 03 CC");
const SIG64_DECOMP_1_3_1: Signature<9> = Signature::new("41 8B 94 95 ?? ?? ?? ?? 49");
const SIG64_DECOMP_1_0_0_LEA: Signature<10> = Signature::new("4C 8D 35 ?? ?? ?? ?? 44 8B 1D"); // Signature::new("4C 8D 35 ?? ?? ?? ?? 66 90");
const SIG64_DECOMP_CENTISECS: Signature<11> = Signature::new("89 0D ?? ?? ?? ?? 41 3B C8 75 3A");
const SIG64_DECOMP_CENTISECS_ALT: Signature<11> =
    Signature::new("89 0D ?? ?? ?? ?? 41 3B C8 75 3E");
const SIG64_DECOMP_TIMERBUG: Signature<14> =
    Signature::new("89 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 63 15");
