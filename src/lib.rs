#![no_std]
use asr::{signature::Signature, timer, timer::TimerState, watcher::Watcher, Address, Process, time::Duration};

#[cfg(all(not(test), target_arch = "wasm32"))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

static AUTOSPLITTER: spinning_top::Spinlock<State> = spinning_top::const_spinlock(State {
    game: None,
    watchers: Watchers {
        accumulated_igt: Duration::ZERO,
        buffer_igt: Duration::ZERO,
        igt_offset: Duration::ZERO,
        time_bonus_start_value: 0,
        demo_mode: Watcher::new(),
        state: Watcher::new(),
        time_bonus: Watcher::new(),
        final_boss_health: Watcher::new(),
        level_id: Watcher::new(),
        timer_is_running: Watcher::new(),
        igt: Watcher::new(),
        centisecs: Watcher::new(),
        livesplit_timer_state: Watcher::new(),
    },
    settings: None,
});

struct State {
    game: Option<ProcessInfo>,
    watchers: Watchers,
    settings: Option<Settings>,
}

struct ProcessInfo {
    game: Process,
    is_64_bit: bool,
    game_version: GameVersion,
    main_module_base: Address,
    main_module_size: u64,
    has_centisecs_bug: bool,
    addresses: Option<MemoryPtr>,
}

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

struct MemoryPtr {
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
}

#[derive(asr::Settings)]
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

impl ProcessInfo {
    pub fn attach_process() -> Option<Self> {
        const PROCESS_NAMES: [&str; 8] = ["soniccd.exe", "RSDKv3.exe", "RSDKv3_64.exe", "RSDKv3_HW.exe", "RSDKv3_HW_64.exe", "Sonic CD.exe", "Sonic CD_64.exe", "Restored.exe"];
        let mut proc: Option<Process> = None;
        let mut proc_name: Option<&str> = None;
    
        for name in PROCESS_NAMES {
            proc = Process::attach(name);
            if proc.is_some() {
                proc_name = Some(name);
                break
            }
        }
    
        let game = proc?;
        let main_module_base = game.get_module_address(proc_name?).ok()?;
        let main_module_size = game.get_module_size(proc_name?).ok()?;

        // Determine game version through signature scanning
        let is_64_bit: bool;
        let game_version: GameVersion;
        let has_centisecs_bug: bool;

        if SIG32_RETAIL.scan_process_range(&game, main_module_base, main_module_size).is_some() {
            is_64_bit = false;
            game_version = GameVersion::Retail;
            has_centisecs_bug = true;
        } else if SIG32_DECOMP_1_0_0.scan_process_range(&game, main_module_base, main_module_size).is_some() {
            is_64_bit = false;
            game_version = GameVersion::Decompilation32bit1_0_0;
            has_centisecs_bug = SIG32_DECOMP_TIMERBUG.scan_process_range(&game, main_module_base, main_module_size).is_none();
        } else if SIG32_DECOMP_1_3_1.scan_process_range(&game, main_module_base, main_module_size).is_some() {
            is_64_bit = false;
            game_version = GameVersion::Decompilation32bit1_3_1;
            has_centisecs_bug = false;
        } else if SIG64_DECOMP_1_0_0.scan_process_range(&game, main_module_base, main_module_size).is_some() {
            is_64_bit = true;
            game_version = GameVersion::Decompilation64bit1_0_0;
            has_centisecs_bug = SIG64_DECOMP_TIMERBUG.scan_process_range(&game, main_module_base, main_module_size).is_none();
        } else if SIG64_DECOMP_1_3_1.scan_process_range(&game, main_module_base, main_module_size).is_some() {
            is_64_bit = true;
            game_version = GameVersion::Decompilation64bit1_3_1;
            has_centisecs_bug = false;
        } else {
            return None
        }
   
        Some(Self {
            game,
            is_64_bit,
            game_version,
            has_centisecs_bug,
            main_module_base,
            main_module_size,
            addresses: None,
        })
    }

    fn look_for_addresses(&mut self) -> Option<MemoryPtr> {
        let game = &self.game;

        let ptr: u64;
        let mut lea: u64 = 0;

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

        match self.game_version {
            GameVersion::Retail => {
                ptr = game.read::<u32>(Address(SIG32_RETAIL.scan_process_range(game, self.main_module_base, self.main_module_size)?.0 + 3)).ok()? as u64;
            },
            GameVersion::Decompilation32bit1_0_0 => {
                ptr = game.read::<u32>(Address(SIG32_DECOMP_1_0_0.scan_process_range(game, self.main_module_base, self.main_module_size)?.0 + 3)).ok()? as u64;
            },
            GameVersion::Decompilation32bit1_3_1 => {
                ptr = game.read::<u32>(Address(SIG32_DECOMP_1_3_1.scan_process_range(game, self.main_module_base, self.main_module_size)?.0 + 3)).ok()? as u64;
            },
            GameVersion::Decompilation64bit1_0_0 => {
                let addr = SIG64_DECOMP_1_0_0.scan_process_range(game, self.main_module_base, self.main_module_size)?.0 + 4;
                ptr = self.main_module_base.0 + game.read::<u32>(Address(addr)).ok()? as u64;

                let addr = SIG64_DECOMP_1_0_0_LEA.scan_process_range(game, self.main_module_base, self.main_module_size)?.0 + 3;
                lea = addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64;
            },
            GameVersion::Decompilation64bit1_3_1 => {
                let addr = SIG64_DECOMP_1_3_1.scan_process_range(game, self.main_module_base, self.main_module_size)?.0 + 4;
                ptr = self.main_module_base.0 + game.read::<u32>(Address(addr)).ok()? as u64;
                
                let addr = SIG64_DECOMP_1_0_0_LEA.scan_process_range(game, self.main_module_base, self.main_module_size)?.0 + 3;
                lea = addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64;
            },
        }

        // Scanning function
        let pointerpath = |offset1: u32, offset2: u32, offset3: u32, absolute: bool| -> Address {
            if self.is_64_bit {
                if offset1 == 0 {
                    return Address(lea + offset3 as u64)
                }
                let temp_offset = game.read::<u32>(Address(ptr + offset1 as u64)).ok().unwrap_or_default();
                let temp_offset2 = self.main_module_base.0 + temp_offset as u64 + offset2 as u64;
                if absolute {
                    Address(self.main_module_base.0 + game.read::<u32>(Address(temp_offset2)).ok().unwrap_or_default() as u64 + offset3 as u64)
                } else {
                    Address(temp_offset2 as u64 + 0x4 + game.read::<u32>(Address(temp_offset2)).ok().unwrap_or_default() as u64 + offset3 as u64)
                }
            } else {
                Address(game.read_pointer_path32::<u32>(ptr as u32 + offset1, &[0, offset2 as u32]).ok().unwrap_or_default() as u64 + offset3 as u64)
            }
        };

        match self.game_version {
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

                let ptr = SIG32_RETAIL_CENTISECS.scan_process_range(game, self.main_module_base, self.main_module_size)?.0;
                centisecs = Address(game.read::<u32>(Address(ptr + 1)).ok()? as u64);
                seconds = Address(game.read::<u32>(Address(ptr + 35)).ok()? as u64);
                minutes = Address(game.read::<u32>(Address(ptr + 69)).ok()? as u64);
            },
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

                let ptr = SIG32_DECOMP_CENTISECS.scan_process_range(game, self.main_module_base, self.main_module_size)?.0;
                centisecs = Address(game.read::<u32>(Address(ptr + 2)).ok()? as u64);
                seconds = Address(game.read::<u32>(Address(ptr + 29)).ok()? as u64);
                minutes = Address(game.read::<u32>(Address(ptr + 51)).ok()? as u64);
            },
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

                let ptr = SIG32_DECOMP_CENTISECS.scan_process_range(game, self.main_module_base, self.main_module_size)?.0;
                centisecs = Address(game.read::<u32>(Address(ptr + 2)).ok()? as u64);
                seconds = Address(game.read::<u32>(Address(ptr + 29)).ok()? as u64);
                minutes = Address(game.read::<u32>(Address(ptr + 51)).ok()? as u64);
            },
            GameVersion::Decompilation64bit1_0_0 | GameVersion::Decompilation64bit1_3_1 => {
                demo_mode = pointerpath(0x4 * 11, 15, 0x1AC, true);
                level_id_type = pointerpath(0x4 * 119, 10, 0, false);
                level_id = pointerpath(0x4 * 120, 10, 0, false);
                timer_is_running = pointerpath(0x4 * 121, 12, 0, false);
                state = pointerpath(0x4 * 0, 0, 0x10B2, false);
                score_tally_state = pointerpath(0x4 * 0, 0, 0x832, false);
                time_bonus = pointerpath(0x4 * 0, 0, 0x814, false);
                bhp_good = pointerpath(0x4 * 0, 0, 0x37D0, false);
                bhp_bad = pointerpath(0x4 * 0, 0, 0x3814, false);

                if let Some(ptr) = SIG64_DECOMP_CENTISECS.scan_process_range(game, self.main_module_base, self.main_module_size) {
                    let mut addr = ptr.0 + 2;
                    centisecs = Address(addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64);
                    addr = ptr.0 + 29;
                    seconds = Address(addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64);
                    addr = ptr.0 + 54;
                    minutes = Address(addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64);
                } else {
                    let ptr = SIG64_DECOMP_CENTISECS_ALT.scan_process_range(game, self.main_module_base, self.main_module_size)?.0;
                    let mut addr = ptr + 2;
                    centisecs = Address(addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64);
                    addr = ptr + 31;
                    seconds = Address(addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64);
                    addr = ptr + 57;
                    minutes = Address(addr + 0x4 + game.read::<u32>(Address(addr)).ok()? as u64);
                }
            },
        };

        Some(MemoryPtr {
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
        })
    }
}

impl State {
    fn init(&mut self) -> bool {        
        if self.game.is_none() {
            self.game = ProcessInfo::attach_process()
        }

        let Some(game) = &mut self.game else {
            return false
        };

        if !game.game.is_open() {
            self.game = None;
            return false
        }

        if game.addresses.is_none() {
            game.addresses = game.look_for_addresses()
        }

        game.addresses.is_some()   
    }

    fn update(&mut self) {
        let Some(game) = &self.game else { return };
        let Some(addresses) = &game.addresses else { return };
        let proc = &game.game;

        // LiveSplit's timer state, defined inside a watcher in order to define some actions when the timer starts or resets
        let Some(timer_state) = self.watchers.livesplit_timer_state.update(Some(timer::state())) else { return };

        // Update standard values
        self.watchers.demo_mode.update(Some(proc.read::<u8>(addresses.demo_mode).ok().unwrap_or_default() > 0));
        self.watchers.state.update(proc.read(addresses.state).ok());
        self.watchers.timer_is_running.update(Some(proc.read::<u8>(addresses.timer_is_running).ok().unwrap_or_default() > 0));

        // Level ID
        match proc.read::<u8>(addresses.score_tally_state).ok().unwrap_or_default() {
            0 => {
                let lid = proc.read::<u8>(addresses.level_id_type).ok().unwrap_or_default() as u32 * 100 + proc.read::<u8>(addresses.level_id).ok().unwrap_or_default() as u32;
                let current_act = match lid {
                    0 => Acts::TitleScreen,
                    1 => Acts::MainMenu,
                    2 => Acts::TimeAttack,
                    8 => Acts::Credits,
                    100 | 101 | 102 | 103 => Acts::PalmtreePanicAct1,
                    104 | 105 | 106 | 107 => Acts::PalmtreePanicAct2,
                    108 | 109 => Acts::PalmtreePanicAct3,
                    110 | 111 | 112 | 113 => Acts::CollisionChaosAct1,
                    114 | 115 | 116 | 117 => Acts::CollisionChaosAct2,
                    118 | 119 => Acts::CollisionChaosAct3,
                    120 | 121 | 122 | 123 => Acts::TidalTempestAct1,
                    124 | 125 | 126 | 127 => Acts::TidalTempestAct2,
                    128 | 129 => Acts::TidalTempestAct3,
                    130 | 131 | 132 | 133 => Acts::QuartzQuadrantAct1,
                    134 | 135 | 136 | 137 => Acts::QuartzQuadrantAct2,
                    138 | 139 => Acts::QuartzQuadrantAct3,
                    140 | 141 | 142 | 143 => Acts::WackyWorkbenchAct1,
                    144 | 145 | 146 | 147 => Acts::WackyWorkbenchAct2,
                    148 | 149 => Acts::WackyWorkbenchAct3,
                    150 | 151 | 152 | 153 => Acts::StardustSpeedwayAct1,
                    154 | 155 | 156 | 157 => Acts::StardustSpeedwayAct2,
                    158 | 159 => Acts::StardustSpeedwayAct3,
                    160 | 161 | 162 | 163 => Acts::MetallicMadnessAct1,
                    164 | 165 | 166 | 167 => Acts::MetallicMadnessAct2,
                    168 | 169 => Acts::MetallicMadnessAct3, 
                    _ => match &self.watchers.level_id.pair { Some(x) => x.current, _ => Acts::PalmtreePanicAct1 },
                };
                self.watchers.level_id.update(Some(current_act));
        
                let final_boss_health = match lid {
                    168 => proc.read::<u8>(addresses.bhp_good).ok().unwrap_or_default(),
                    169 => proc.read::<u8>(addresses.bhp_bad).ok().unwrap_or_default(),
                    _ => 0xFF,
                };
                self.watchers.final_boss_health.update(Some(final_boss_health));
            },
            _ => {
                self.watchers.level_id.update(Some(match &self.watchers.level_id.pair {
                    Some(x) => x.current,
                    _ => Acts::PalmtreePanicAct1,
                }));
                self.watchers.final_boss_health.update(Some(0xFF));
            },
        };

        // IGT logic
        let Some(demo_mode) = &self.watchers.demo_mode.pair else { return };
        let Some(timer_is_running) = &self.watchers.timer_is_running.pair else { return };

        let centisecs = (proc.read::<u8>(addresses.centisecs).ok().unwrap_or_default() as u64 * 100) / 60;
        let Some(centis) = self.watchers.centisecs.update(Some(Duration::milliseconds(centisecs as i64 * 10))) else { return };

        let new_igt = if demo_mode.current || demo_mode.old || timer_state.current == TimerState::NotRunning {
            Duration::ZERO
        } else if !timer_is_running.old && !timer_is_running.current {
            match &self.watchers.igt.pair {
                Some(x) => x.current,
                _ => Duration::ZERO
            }
        } else {
            let mins = proc.read::<u8>(addresses.minutes).ok().unwrap_or_default() as u64;
            let secs = proc.read::<u8>(addresses.seconds).ok().unwrap_or_default() as u64;
            Duration::milliseconds((mins * 60000 + secs * 1000 + if game.has_centisecs_bug { 0 } else { centisecs } * 10) as i64)
        };
        let Some(final_igt) = self.watchers.igt.update(Some(new_igt)) else { return };

        // Reset the buffer IGT variables when the timer is stopped
        if timer_state.current == TimerState::NotRunning {
            self.watchers.accumulated_igt = Duration::ZERO;
            self.watchers.buffer_igt = Duration::ZERO;
            self.watchers.igt_offset = Duration::ZERO;
        }

        if final_igt.old > final_igt.current {
            self.watchers.accumulated_igt += final_igt.old - self.watchers.buffer_igt;
            self.watchers.buffer_igt = final_igt.current;
        }

        // Set the IGT offset when starting a new run, if the game has the centisecs bug
        if game.has_centisecs_bug && timer_state.old == TimerState::NotRunning && timer_state.current == TimerState::Running {
            self.watchers.igt_offset = centis.current;
        }

        // Time bonus start value
        let Some(time_bonus) = self.watchers.time_bonus.update(proc.read::<u32>(addresses.time_bonus).ok()) else { return };
        if time_bonus.old == 0 && time_bonus.changed() {
            self.watchers.time_bonus_start_value = time_bonus.current
        } else if time_bonus.current == 0 {
            self.watchers.time_bonus_start_value = 0
        }
    }

    fn start(&mut self) -> bool {
        let Some(settings) = &self.settings else { return false };
        if !settings.start { return false }
        let Some(act) = &self.watchers.level_id.pair else { return false };
        let Some(state) = &self.watchers.state.pair else { return false };
        act.current == Acts::MainMenu && state.current == 7 && state.old == 6
    }

    fn split(&mut self) -> bool {
        let Some(settings) = &self.settings else { return false };
        let Some(act) = &self.watchers.level_id.pair else {return false };

        match act.old {
            Acts::PalmtreePanicAct1 => settings.palmtree_panic_1 && act.current == Acts::PalmtreePanicAct2,
            Acts::PalmtreePanicAct2 => settings.palmtree_panic_2 && act.current == Acts::PalmtreePanicAct3,
            Acts::PalmtreePanicAct3 => settings.palmtree_panic_3 && act.current == Acts::CollisionChaosAct1,
            Acts::CollisionChaosAct1 => settings.collision_chaos_1 && act.current == Acts::CollisionChaosAct2,
            Acts::CollisionChaosAct2 => settings.collision_chaos_2 && act.current == Acts::CollisionChaosAct3,
            Acts::CollisionChaosAct3 => settings.collision_chaos_3 && act.current == Acts::TidalTempestAct1,
            Acts::TidalTempestAct1 => settings.tidal_tempest_1 && act.current == Acts::TidalTempestAct2,
            Acts::TidalTempestAct2 => settings.tidal_tempest_2 && act.current == Acts::TidalTempestAct3,
            Acts::TidalTempestAct3 => settings.tidal_tempest_3 && act.current == Acts::QuartzQuadrantAct1,
            Acts::QuartzQuadrantAct1 => settings.quartz_quadrant_1 && act.current == Acts::QuartzQuadrantAct2,
            Acts::QuartzQuadrantAct2 => settings.quartz_quadrant_2 && act.current == Acts::QuartzQuadrantAct3,
            Acts::QuartzQuadrantAct3 => settings.quartz_quadrant_3 && act.current == Acts::WackyWorkbenchAct1,
            Acts::WackyWorkbenchAct1 => settings.wacky_workbench_1 && act.current == Acts::WackyWorkbenchAct2,
            Acts::WackyWorkbenchAct2 => settings.wacky_workbench_2 && act.current == Acts::WackyWorkbenchAct3,
            Acts::WackyWorkbenchAct3 => settings.wacky_workbench_3 && act.current == Acts::StardustSpeedwayAct1,
            Acts::StardustSpeedwayAct1 => settings.stardust_speedway_1 && act.current == Acts::StardustSpeedwayAct2,
            Acts::StardustSpeedwayAct2 => settings.stardust_speedway_2 && act.current == Acts::StardustSpeedwayAct3,
            Acts::StardustSpeedwayAct3 => settings.stardust_speedway_3 && act.current == Acts::MetallicMadnessAct1,
            Acts::MetallicMadnessAct1 => settings.metallic_madness_1 && act.current == Acts::MetallicMadnessAct2,
            Acts::MetallicMadnessAct2 => settings.metallic_madness_2 && act.current == Acts::MetallicMadnessAct3,
            Acts::MetallicMadnessAct3 => settings.metallic_madness_3 && {
                let Some(finalboss_hp) = &self.watchers.final_boss_health.pair else { return false };
                let Some(igt) = &self.watchers.igt.pair else { return false };
                if settings.rta_tb {
                    (act.current == Acts::Credits || act.current == Acts::MainMenu) && finalboss_hp.old == 0 && igt.old != Duration::ZERO
                } else {
                    finalboss_hp.old == 1 && finalboss_hp.current == 0 && igt.current != Duration::ZERO
                }
            },
            _ => false,
        }
    }

    fn reset(&mut self) -> bool {
        let Some(settings) = &self.settings else { return false };
        if !settings.reset { return false }
        let Some(act) = &self.watchers.level_id.pair else { return false };
        let Some(state) = &self.watchers.state.pair else { return false };
        act.current == Acts::MainMenu && state.current == 5 && state.changed()
    }

    fn is_loading(&mut self) -> Option<bool> {
        let Some(settings) = &self.settings else { return None };
        if settings.rta_tb {
            let Some(time_bonus) = &self.watchers.time_bonus.pair else { return None };
            Some(self.watchers.time_bonus_start_value != 0 && time_bonus.current != self.watchers.time_bonus_start_value)
        } else {
            Some(true)
        }
    }

    fn game_time(&mut self) -> Option<Duration> {
        let Some(settings) = &self.settings else { return None };
        if settings.rta_tb {
            None
        } else {
            let Some(igt) = &self.watchers.igt.pair else { return None };
            let Some(centisecs) = &self.watchers.centisecs.pair else { return None };
            let Some(game) = &self.game else { return None };
            Some(igt.current + self.watchers.accumulated_igt - self.watchers.buffer_igt - self.watchers.igt_offset + if game.has_centisecs_bug { centisecs.current } else { Duration::ZERO })
        }
    }
}

#[no_mangle]
pub extern "C" fn update() {
    // Get access to the spinlock
    let autosplitter = &mut AUTOSPLITTER.lock();
    
    // Sets up the settings
    autosplitter.settings.get_or_insert_with(Settings::register);

    // Main autosplitter logic, essentially refactored from the OG LivaSplit autosplitting component.
    // First of all, the autosplitter needs to check if we managed to attach to the target process,
    // otherwise there's no need to proceed further.
    if !autosplitter.init() {
        return
    }

    // The main update logic is launched with this
    autosplitter.update();

    // Splitting logic. Adapted from OG LiveSplit:
    // Order of execution
    // 1. update() [this is launched above] will always be run first. There are no conditions on the execution of this action.
    // 2. If the timer is currently either running or paused, then the isLoading, gameTime, and reset actions will be run.
    // 3. If reset does not return true, then the split action will be run.
    // 4. If the timer is currently not running (and not paused), then the start action will be run.
    let timer_state = timer::state();
    if timer_state == TimerState::Running || timer_state == TimerState::Paused {
        if let Some(is_loading) = autosplitter.is_loading() {
            if is_loading {
                timer::pause_game_time()
            } else {
                timer::resume_game_time()
            }
        }

        if let Some(game_time) = autosplitter.game_time() {
            timer::set_game_time(game_time)
        }

        if autosplitter.reset() {
            timer::reset()
        } else if autosplitter.split() {
            timer::split()
        }
    } 

    if timer::state() == TimerState::NotRunning {
        if autosplitter.start() {
            timer::start();

            if let Some(is_loading) = autosplitter.is_loading() {
                if is_loading {
                    timer::pause_game_time()
                } else {
                    timer::resume_game_time()
                }
            }
        }
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


const SIG32_RETAIL: Signature<13> = Signature::new("FF 24 85 ?? ?? ?? ?? 8B 4D F0 8B 14 8D");
const SIG32_RETAIL_CENTISECS: Signature<15> = Signature::new("A2 ?? ?? ?? ?? 0F B6 0D ?? ?? ?? ?? 83 F9 3C");

const SIG32_DECOMP_1_0_0: Signature<10> = Signature::new("FF 24 85 ?? ?? ?? ?? 8B 04 B5");
const SIG32_DECOMP_1_3_1: Signature<10> = Signature::new("FF 24 8D ?? ?? ?? ?? 8B 0C 85");
const SIG32_DECOMP_CENTISECS: Signature<8> = Signature::new("89 0D ?? ?? ?? ?? 3B CE");
const SIG32_DECOMP_TIMERBUG: Signature<34> = Signature::new("C6 05 ?? ?? ?? ?? 00 C6 05 ?? ?? ?? ?? 00 C7 05 ?? ?? ?? ?? 00 00 00 00 C7 05 ?? ?? ?? ?? 00 00 00 00");

const SIG64_DECOMP_1_0_0: Signature<11> = Signature::new("41 8B 8C 8C ?? ?? ?? ?? 49 03 CC");
const SIG64_DECOMP_1_3_1: Signature<9> = Signature::new("41 8B 94 95 ?? ?? ?? ?? 49");
const SIG64_DECOMP_1_0_0_LEA: Signature<10> = Signature::new("4C 8D 35 ?? ?? ?? ?? 44 8B 1D"); // Signature::new("4C 8D 35 ?? ?? ?? ?? 66 90");
const SIG64_DECOMP_CENTISECS: Signature<11> = Signature::new("89 0D ?? ?? ?? ?? 41 3B C8 75 3A");
const SIG64_DECOMP_CENTISECS_ALT: Signature<11> = Signature::new("89 0D ?? ?? ?? ?? 41 3B C8 75 3E");
const SIG64_DECOMP_TIMERBUG: Signature<14> = Signature::new("89 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 63 15");