use tm_replay::*;
mod compress;
mod char_data;
use std::env;
use std::fs;


pub fn funstruct_tm_replay(
    json_bytes: &[u8],
    state: &RecordingState, 
    inputs: &InputRecordings,
    flags: ReplayFlags,
) -> Result<Vec<u8>, ReplayCreationError> {
    if state.cpu_state.character.character() == slp_parser::Character::Zelda { 
        return Err(ReplayCreationError::ZeldaOnCpu) 
    }

    //if let slp_parser::ActionState::Special(_) = state.hmn_state.state {
    //    return Err(ReplayCreationError::SpecialActionState)
    //}

    //if let slp_parser::ActionState::Special(_) = state.cpu_state.state {
    //    return Err(ReplayCreationError::SpecialActionState)
    //}

    // buffer created by unclepunch's tm code
    let mut bytes = Vec::with_capacity(8192 * 8);

    state.write_header(&mut bytes, flags & replay_flags::SWAP_SHEIK_ZELDA != 0);

    //let mut image = include_bytes!("/home/alex/Downloads/test_image_rgb565.bin");
    //let mut image = [0u8; 2*96*72];
    //for i in 0..image.len() {
    //    let b1 = 0b11010111;
    //    image[i] = b1;
    //}

    let screenshot_offset = bytes.len();
    let screenshot_size = 2 * 96 * 72;
    bytes.resize(68 + screenshot_size, 0u8); // black screen for now
    //tes.extend_from_slice(image);

    //bytes[screenshot_offset..screenshot_offset + screenshot_size].fill(0x11);

    //let message = b"HELLO FROM SCREENSHOT!";
    let msg_len = json_bytes.len().min(screenshot_size);
    println!( "json bytes: {}", msg_len);
    //bytes[screenshot_offset + 1] = msg_len;
    bytes[screenshot_offset + 2..screenshot_offset + msg_len + 2].copy_from_slice(&json_bytes[..msg_len]);

    let recording_offset = bytes.len();

    let mut recording_save = vec![0u8; RECORDING_SIZE + 257]; // pad a bit for compression algo
    let rec_start = SAVESTATE_SIZE+MATCHINIT_SIZE;
    recording_save[0..rec_start].copy_from_slice(&DEFAULT_SAVESTATE_AND_MATCHINIT[..rec_start]);

    let savestate_offset = MATCHINIT_SIZE;
    recording_save[savestate_offset+4..][..4].copy_from_slice(&state.start_frame.to_be_bytes());

    // overwrite MatchInit values

    let stage = state.stage.to_u16_external();
    recording_save[0x0E..][..2].copy_from_slice(&stage.to_be_bytes());

    // write FtState values

    fn write_ft_state(ft_state: &mut [u8], st: &CharacterState, follower: Option<&CharacterState>) {
        let ft_savestate_data_size = 4396;
        let playerblock_offset = ft_savestate_data_size*2;
        let stale_offset = 8972;

        write_ft_save_state_data(ft_state, st);
        if let Some(follower_st) = follower { 
            write_ft_save_state_data(&mut ft_state[ft_savestate_data_size..], follower_st);
        }

        // stale moves ------------------------------------

        let stale_move_next_idx = st.stale_moves.iter()
            .position(|st| st.attack == slp_parser::AttackKind::Null)
            .unwrap_or(0) as u32;
        ft_state[stale_offset..][..4].copy_from_slice(&stale_move_next_idx.to_be_bytes());

        for i in 0..10 {
            let offset = stale_offset + 4 + 4*i;
            let st = st.stale_moves[i];
            ft_state[offset+1..][..1].copy_from_slice(&(st.attack as u8).to_be_bytes());
            ft_state[offset+2..][..2].copy_from_slice(&st.instance_id.to_be_bytes());
        }

        // Playerblock ---------------------------------

        // fix stock icons
        let character = st.character.character().to_u8_external().unwrap();
        let costume = st.character.costume_idx();
        ft_state[playerblock_offset..][4..8].copy_from_slice(&(character as u32).to_be_bytes());
        ft_state[playerblock_offset..][68] = costume;
    }
    
    fn write_ft_save_state_data(ft_state: &mut [u8], st: &CharacterState) {
        // nested struct offsets
        let phys_offset = 40;
        let input_offset = 568;
        let collision_offset = 676; // CollData
        let camera_box_offset = 1092; // CameraBox
        let flags_offset = 3356;
        let char_fighter_var_offset = 3384;
        let char_state_var_offset = 3592;
        let subaction_flags_offset = 3664;
        let dmg_offset = 3680;
        let grab_offset = 3988;
        let jump_offset = 4048;
        let smash_offset = 4052;
        let hurt_offset = 4092;

        // state, direction, anim frame, anim speed, anim blend
        let state_offset = 4;
        ft_state[state_offset..][..4].copy_from_slice(&(st.state.as_u16() as u32).to_be_bytes());
        let direction_bytes = match st.direction {
            slp_parser::Direction::Left => (-1.0f32).to_be_bytes(),
            slp_parser::Direction::Right => 1.0f32.to_be_bytes(),
        };
        ft_state[state_offset..][4..8].copy_from_slice(&direction_bytes);
        ft_state[state_offset..][8..12].copy_from_slice(&st.state_frame.to_be_bytes());
        ft_state[state_offset..][12..16].copy_from_slice(&st.state_speed.to_be_bytes());
        ft_state[state_offset..][16..20].copy_from_slice(&st.state_blend.to_be_bytes());

        // idk
        ft_state[state_offset..][20..24].copy_from_slice(&(st.x_rotn_rot[0]).to_be_bytes());
        ft_state[state_offset..][24..28].copy_from_slice(&(st.x_rotn_rot[1]).to_be_bytes());
        ft_state[state_offset..][28..32].copy_from_slice(&(st.x_rotn_rot[2]).to_be_bytes());
        ft_state[state_offset..][32..36].copy_from_slice(&(st.x_rotn_rot[3]).to_be_bytes());

        // phys struct -------------------------
        
        // velocities
        ft_state[phys_offset..][0..4].copy_from_slice(&st.anim_velocity[0].to_be_bytes()); // anim_vel.x
        ft_state[phys_offset..][4..8].copy_from_slice(&st.anim_velocity[1].to_be_bytes()); // anim_vel.y
        ft_state[phys_offset..][8..12].copy_from_slice(&st.anim_velocity[2].to_be_bytes()); // anim_vel.z
        ft_state[phys_offset..][12..16].copy_from_slice(&st.self_velocity[0].to_be_bytes()); // self_vel.x
        ft_state[phys_offset..][16..20].copy_from_slice(&st.self_velocity[1].to_be_bytes()); // self_vel.y
        ft_state[phys_offset..][20..24].copy_from_slice(&st.self_velocity[2].to_be_bytes()); // self_vel.z
        ft_state[phys_offset..][24..28].copy_from_slice(&st.hit_velocity[0].to_be_bytes()); // kb_vel.x
        ft_state[phys_offset..][28..32].copy_from_slice(&st.hit_velocity[1].to_be_bytes()); // kb_vel.y
        ft_state[phys_offset..][32..36].copy_from_slice(&st.hit_velocity[2].to_be_bytes()); // kb_vel.z
        ft_state[phys_offset..][120..124].copy_from_slice(&st.ground_velocity[0].to_be_bytes()); // selfVelGround.x
        ft_state[phys_offset..][124..128].copy_from_slice(&st.ground_velocity[1].to_be_bytes()); // selfVelGround.y
        ft_state[phys_offset..][128..132].copy_from_slice(&st.ground_velocity[2].to_be_bytes()); // selfVelGround.z

        // position
        ft_state[phys_offset..][60..64].copy_from_slice(&st.position[0].to_be_bytes()); // pos.x
        ft_state[phys_offset..][64..68].copy_from_slice(&st.position[1].to_be_bytes()); // pos.y
        ft_state[phys_offset..][68..72].copy_from_slice(&st.position[2].to_be_bytes()); // pos.z
        ft_state[phys_offset..][72..76].copy_from_slice(&st.prev_position[0].to_be_bytes()); // pos_prev.x
        ft_state[phys_offset..][76..80].copy_from_slice(&st.prev_position[1].to_be_bytes()); // pos_prev.y
        ft_state[phys_offset..][80..84].copy_from_slice(&st.prev_position[2].to_be_bytes()); // pos_prev.z
        ft_state[phys_offset..][84..88].copy_from_slice(&(0.0f32).to_be_bytes()); // pos_delta.x
        ft_state[phys_offset..][88..92].copy_from_slice(&(0.0f32).to_be_bytes()); // pos_delta.y
        ft_state[phys_offset..][92..96].copy_from_slice(&(0.0f32).to_be_bytes()); // pos_delta.z

        ft_state[phys_offset..][108..112].copy_from_slice(&(st.airborne as u32).to_be_bytes());
        
        // input struct -----------------

        ft_state[input_offset..][0..4].copy_from_slice(&st.stick[0].to_be_bytes());
        ft_state[input_offset..][4..8].copy_from_slice(&st.stick[1].to_be_bytes());
        ft_state[input_offset..][8..12].copy_from_slice(&st.prev_stick[0].to_be_bytes());
        ft_state[input_offset..][12..16].copy_from_slice(&st.prev_stick[1].to_be_bytes());
        ft_state[input_offset..][24..28].copy_from_slice(&st.cstick[0].to_be_bytes());
        ft_state[input_offset..][28..32].copy_from_slice(&st.cstick[1].to_be_bytes());
        ft_state[input_offset..][48..52].copy_from_slice(&st.trigger.to_be_bytes());

        ft_state[input_offset..][60..64].copy_from_slice(&(st.held as u32).to_be_bytes());
        ft_state[input_offset..][64..68].copy_from_slice(&(st.prev_held as u32).to_be_bytes());
        ft_state[input_offset..][72..76].copy_from_slice(&((st.prev_held & st.held) as u32).to_be_bytes());

        ft_state[input_offset..][0x50] = st.input_timers.timer_lstick_tilt_x;            
        ft_state[input_offset..][0x51] = st.input_timers.timer_lstick_tilt_y;            
        ft_state[input_offset..][0x52] = st.input_timers.timer_trigger_analog;           
        ft_state[input_offset..][0x53] = st.input_timers.timer_lstick_smash_x;           
        ft_state[input_offset..][0x54] = st.input_timers.timer_lstick_smash_y;           
        ft_state[input_offset..][0x55] = st.input_timers.timer_trigger_digital;          
        ft_state[input_offset..][0x56] = st.input_timers.timer_lstick_any_x;             
        ft_state[input_offset..][0x57] = st.input_timers.timer_lstick_any_y;             
        ft_state[input_offset..][0x58] = st.input_timers.timer_trigger_any;              
        ft_state[input_offset..][0x59] = st.input_timers.x679_x;                         
        ft_state[input_offset..][0x5A] = st.input_timers.x67A_y;                         
        ft_state[input_offset..][0x5B] = st.input_timers.x67B;                           
        ft_state[input_offset..][0x5C] = st.input_timers.timer_a;                        
        ft_state[input_offset..][0x5D] = st.input_timers.timer_b;                        
        ft_state[input_offset..][0x5E] = st.input_timers.timer_xy;                       
        ft_state[input_offset..][0x5F] = st.input_timers.timer_trigger_any_ignore_hitlag;
        ft_state[input_offset..][0x60] = st.input_timers.timer_LR;                       
        ft_state[input_offset..][0x61] = st.input_timers.timer_padup;                    
        ft_state[input_offset..][0x62] = st.input_timers.timer_paddown;                  
        ft_state[input_offset..][0x63] = st.input_timers.timer_item_release;             
        ft_state[input_offset..][0x64] = st.input_timers.since_rapid_lr;                 
        ft_state[input_offset..][0x65] = st.input_timers.timer_jump;                     
        ft_state[input_offset..][0x66] = st.input_timers.timer_specialhi;                
        ft_state[input_offset..][0x67] = st.input_timers.timer_speciallw;                
        ft_state[input_offset..][0x68] = st.input_timers.timer_specials;                 
        ft_state[input_offset..][0x69] = st.input_timers.timer_specialn;                 
        ft_state[input_offset..][0x6A] = st.input_timers.timer_jump_lockout;             
        ft_state[input_offset..][0x6B] = st.input_timers.timer_specialhi_lockout;        

        let percent_bytes = (st.percent*0.5).to_be_bytes(); // percent is stored halved for some reason???
        ft_state[dmg_offset..][4..8].copy_from_slice(&percent_bytes); // percent
        ft_state[dmg_offset..][12..16].copy_from_slice(&percent_bytes); // temp percent???
        ft_state[dmg_offset..][0x80..0x84].copy_from_slice(&st.frames_since_hit.to_be_bytes()); // frames in knockback
        ft_state[dmg_offset..][0xE4..0xE8].copy_from_slice(&st.offscreen_damage_timer.to_be_bytes());
        
        // collision data (CollData) ------------------

        // I believe these set the centre of the ECB.
        // topN_Curr
        ft_state[collision_offset..][4..8].copy_from_slice(&st.position[0].to_be_bytes());
        ft_state[collision_offset..][8..12].copy_from_slice(&st.position[1].to_be_bytes());
        ft_state[collision_offset..][12..16].copy_from_slice(&st.position[2].to_be_bytes());
        // topN_CurrCorrect
        ft_state[collision_offset..][16..20].copy_from_slice(&st.position[0].to_be_bytes());
        ft_state[collision_offset..][20..24].copy_from_slice(&st.position[1].to_be_bytes());
        ft_state[collision_offset..][24..28].copy_from_slice(&st.position[2].to_be_bytes());
        // topN_Prev
        ft_state[collision_offset..][28..32].copy_from_slice(&st.prev_position[0].to_be_bytes());
        ft_state[collision_offset..][32..36].copy_from_slice(&st.prev_position[1].to_be_bytes());
        ft_state[collision_offset..][36..40].copy_from_slice(&st.prev_position[2].to_be_bytes());
        // topN_Proj
        ft_state[collision_offset..][40..44].copy_from_slice(&st.position[0].to_be_bytes());
        ft_state[collision_offset..][44..48].copy_from_slice(&st.position[1].to_be_bytes());
        ft_state[collision_offset..][48..52].copy_from_slice(&st.position[2].to_be_bytes());
        
        let internal_kind = st.character.character().to_u8_internal() as usize;
        let (cliffgrab_width, cliffgrab_y_offset, cliffgrab_height) = char_data::CLIFFGRAB[internal_kind];
        ft_state[collision_offset..][0x54..][..4].copy_from_slice(&cliffgrab_width.to_be_bytes());
        ft_state[collision_offset..][0x58..][..4].copy_from_slice(&cliffgrab_y_offset.to_be_bytes());
        ft_state[collision_offset..][0x5C..][..4].copy_from_slice(&cliffgrab_height.to_be_bytes());

        if st.airborne {
            let ecb_bottom: f32 = if st.self_velocity[1] > 0.0 {
                // if we are rising, then we want to prevent aerial interrupts,
                // so we set the ecb bottom to be very low.
                0.0
            } else {
                // if we are falling, then we want to prevent clipping through the ground,
                // so we set the ecb bottom to be very high.
                4.0
            };
            ft_state[collision_offset..][0xB0..][..4].copy_from_slice(&ecb_bottom.to_be_bytes());
        }
        
        ft_state[collision_offset..][0x14c..][..4].copy_from_slice(&st.last_ground_idx.to_be_bytes());

        // camera data (CameraBox) -------------------------------------
        
        ft_state[camera_box_offset..][0..4].copy_from_slice(&[0u8; 4]); // alloc
        ft_state[camera_box_offset..][4..8].copy_from_slice(&[0u8; 4]); // next box ptr
        // cam pos
        ft_state[camera_box_offset..][16..20].copy_from_slice(&st.position[0].to_be_bytes());
        ft_state[camera_box_offset..][20..24].copy_from_slice(&st.position[1].to_be_bytes());
        ft_state[camera_box_offset..][24..28].copy_from_slice(&st.position[2].to_be_bytes());
        // bone pos (necessary - causes character culling otherwise)
        ft_state[camera_box_offset..][28..32].copy_from_slice(&st.position[0].to_be_bytes());
        ft_state[camera_box_offset..][32..36].copy_from_slice(&st.position[1].to_be_bytes());
        ft_state[camera_box_offset..][36..40].copy_from_slice(&st.position[2].to_be_bytes());

        // hitlag & hitstun handling -----------------------------

        if st.hitlag_frames_left > 0.0 {
            ft_state[dmg_offset..][304..308].copy_from_slice(&st.hitlag_frames_left.to_be_bytes());
            ft_state[flags_offset..][9] = 4;  // hitstop flag
        }

        // flags ----------------------------------------------

        if matches!(
            st.state,
            slp_parser::ActionState::Standard(slp_parser::StandardActionState::Catch | slp_parser::StandardActionState::CatchDash)
        ) {
            // if not set, grabs in progress will always whiff.
            ft_state[flags_offset..][14] = 0x1A; // 0x19 -> 0x1A
        }

        ft_state[flags_offset..][8] = st.state_flags[0];
        ft_state[flags_offset..][10] = st.state_flags[1];
        ft_state[flags_offset..][11] = st.state_flags[2];
        ft_state[flags_offset..][12] = st.state_flags[3];
        ft_state[flags_offset..][15] = st.state_flags[4];

        ft_state[flags_offset..][24] &= !1;         
        ft_state[flags_offset..][24] |= match st.last_lstick_x_direction {
            slp_parser::Direction::Left => 0,
            slp_parser::Direction::Right => 1,
        };

        // multijump flag
        if matches!(
            st.character.character(), 
            slp_parser::Character::Jigglypuff | slp_parser::Character::Kirby
        ) {
            ft_state[flags_offset..][18] |= 0x40;
        } else {
            ft_state[flags_offset..][18] &= !0x40;
        }
        
        // cargo throw flag ?
        if st.character.character() == slp_parser::Character::DonkeyKong {
            ft_state[flags_offset..][18] |= 0x80;
        } else {
            ft_state[flags_offset..][18] &= !0x80;
        }

        // walljump flag
        if matches!(
            st.character.character(),
            slp_parser::Character::Mario 
            | slp_parser::Character::CaptainFalcon
            | slp_parser::Character::Falco
            | slp_parser::Character::Fox
            | slp_parser::Character::Samus
            | slp_parser::Character::Sheik
            | slp_parser::Character::YoungLink
            | slp_parser::Character::Pichu
        ) {
            ft_state[flags_offset..][20] |= 0x01;
        } else {
            ft_state[flags_offset..][20] &= !0x01;
        }

        ft_state[char_fighter_var_offset..][0..208].copy_from_slice(&st.char_fighter_var);
        ft_state[char_state_var_offset..][0..72].copy_from_slice(&st.char_state_var);
        ft_state[subaction_flags_offset..][0..16].copy_from_slice(&st.subaction_flags);

        // struct grab ----------------------------------------

        let internal_kind = st.character.character().to_u8_internal() as usize;
        let (grab_release_x, grab_release_y) = char_data::GRAB_RELEASE_POS[internal_kind];
        ft_state[grab_offset..][0x28..][..4].copy_from_slice(&grab_release_x.to_be_bytes());
        ft_state[grab_offset..][0x2C..][..4].copy_from_slice(&grab_release_y.to_be_bytes());
        
        // struct jump ----------------------------------------

        ft_state[jump_offset..][0] = jump_count(st.character.character())- st.jumps_remaining;
        
        // struct smash ----------------------------------------
        
        ft_state[smash_offset..][0..][..4].copy_from_slice(&(st.smash_attack.state as u32).to_be_bytes());
        ft_state[smash_offset..][4..][..4].copy_from_slice(&st.smash_attack.held_frames.to_be_bytes());
        ft_state[smash_offset..][8..][..4].copy_from_slice(&(60f32).to_be_bytes());
        ft_state[smash_offset..][12..][..4].copy_from_slice(&(1.367f32).to_be_bytes());
        ft_state[smash_offset..][16..][..4].copy_from_slice(&(1.0f32).to_be_bytes());
        ft_state[smash_offset..][36..][..4].copy_from_slice(&(1.0f32).to_be_bytes());
        
        // struct hurt ----------------------------------------
        
        let kind = if st.intang_ledge != 0 {
            2u32
        } else if st.intang_respawn != 0 {
            1u32
        } else {
            0u32
        };
        
        ft_state[hurt_offset..][4..][..4].copy_from_slice(&kind.to_be_bytes());
        ft_state[hurt_offset..][8..][..4].copy_from_slice(&st.intang_ledge.to_be_bytes());
        ft_state[hurt_offset..][12..][..4].copy_from_slice(&st.intang_respawn.to_be_bytes());
        
        // callbacks (struct cb) ------------------------------

        let fns_idx = (st.state.as_u16() as usize) * 0x20;

        let fns = if fns_idx < ACTION_FN_LOOKUP_TABLE.len() {
            &ACTION_FN_LOOKUP_TABLE[fns_idx..][..0x20]
        } else {
            let c = st.character.character().to_u8_internal() as usize;
            let offset = SPECIAL_ACTION_FN_CHARACTER_OFFSETS[c] as usize;
            let special_fns_idx = offset + (fns_idx - ACTION_FN_LOOKUP_TABLE.len());
            &SPECIAL_ACTION_FN_LOOKUP_TABLE[special_fns_idx..][..0x20]
        };

        ft_state[0x10CC..][0..4].copy_from_slice(&fns[16..20]); // IASA
        ft_state[0x10CC..][4..8].copy_from_slice(&fns[12..16]); // Anim
        ft_state[0x10CC..][8..20].copy_from_slice(&fns[20..32]); // Phys, Coll, Cam
    }

    let st_offset = 312; // savestate offset - skip MatchInit in RecordingSave
    let ft_state_offset = 8+EVENT_DATASIZE; // FtState array offset - fields in Savestate;
    let ft_state_size = 9016;
    write_ft_state(
        &mut recording_save[st_offset+ft_state_offset..][..ft_state_size],
        &state.hmn_state,
        state.hmn_follower_state.as_ref(),
    );
    write_ft_state(
        &mut recording_save[st_offset+ft_state_offset+ft_state_size..][..ft_state_size],
        &state.cpu_state,
        state.cpu_follower_state.as_ref(),
    );

    // write inputs

    fn write_inputs(slot: &mut [u8], start_frame: i32, inputs: Option<&[Input]>) -> Result<(), ReplayCreationError> {
        if let Some(i) = inputs { 
            if i.len() > 3600 { return Err(ReplayCreationError::DurationTooLong) } 
        }

        // if None or len == 0
        if !inputs.is_some_and(|i| !i.is_empty()) {
           slot[0..4].copy_from_slice(&(-1i32).to_be_bytes()); // start_frame
           slot[4..8].copy_from_slice(&0u32.to_be_bytes());    // num_frames
           return Ok(());
        }

        let inputs = inputs.unwrap();


        //slot.fill(0x11);

        slot[0..4].copy_from_slice(&start_frame.to_be_bytes()); // start frame
        slot[4..8].copy_from_slice(&(inputs.len() as u32).to_be_bytes());    // num_frames
        //slot[4..8].copy_from_slice(&(60*60 as u32).to_be_bytes());    // num_frames


        for frame in 0..inputs.len() {
            let offset = 8 + frame*6;
            let input = inputs[frame];

            slot[offset..offset+6].copy_from_slice(&[
                input.button_flags,
                input.stick_x as u8,
                input.stick_y as u8,
                input.cstick_x as u8,
                input.cstick_y as u8,
                input.trigger,
            ]);
        }

        Ok(())
    }

    // hmn inputs
    for i in 0..REC_SLOTS {
        let input_data_start = rec_start + i*REC_SLOT_SIZE;
        let slot = &mut recording_save[input_data_start..][..REC_SLOT_SIZE];
        write_inputs(slot, state.start_frame, inputs.hmn_slots[i])?;
    }

    // cpu inputs
    for i in 0..REC_SLOTS {
        let input_data_start = rec_start + (i+6)*REC_SLOT_SIZE;
        let slot = &mut recording_save[input_data_start..][..REC_SLOT_SIZE];
        write_inputs(slot, state.start_frame, inputs.cpu_slots[i])?;
    }


    //let input_data_start = rec_start + (i+6)*REC_SLOT_SIZE;
    //let input_data_start = rec_start; // slot 0
    //let slot = &mut recording_save[input_data_start..][..REC_SLOT_SIZE];

    // Fill entire slot with 0x11
    //slot.fill(0x11);
    //let custom_msg = b"I AM TESTING HERE!";
    //let input_data_start = rec_start; // slot 0

    //let slot = &mut recording_save[input_data_start..][..REC_SLOT_SIZE];

    // Place message after 8-byte input header
    //slot[8..8 + custom_msg.len()].copy_from_slice(custom_msg);


    // compress
    bytes.resize(recording_offset + RECORDING_SIZE, 0u8);
    let recording_compressed_size = compress::lz77_compress(
        &recording_save, 
        RECORDING_SIZE as u32, 
        &mut bytes[recording_offset..]
    ) as usize;
    bytes.resize(recording_offset+recording_compressed_size, 0u8);

    let menu_settings_offset = bytes.len();

    state.write_menu_settings(&mut bytes);

    bytes[56..60].copy_from_slice(&(screenshot_offset as u32).to_be_bytes());
    bytes[60..64].copy_from_slice(&(recording_offset as u32).to_be_bytes());
    bytes[64..68].copy_from_slice(&(menu_settings_offset as u32).to_be_bytes());

    //bytes[screenshot_offset..screenshot_offset + screenshot_size].fill(0x11);

    construct_tm_replay_from_replay_buffer(state.time, &state.filename, &bytes)
}





use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Workout {
    name: String,
    workout_type: u8,
    exercises: Vec<String>,
}

use std::path::Path;

fn encode_workouts_to_bytes<P: AsRef<Path>>(json_path: P) -> anyhow::Result<Vec<u8>> {
    let json_content = fs::read_to_string(json_path)?;
    let workouts: Vec<Workout> = serde_json::from_str(&json_content)?;

    let mut buf = Vec::new();
    buf.push(workouts.len() as u8); // Number of workouts

    for workout in workouts {
        let name_bytes = workout.name.as_bytes();
        if name_bytes.len() > 255 {
            anyhow::bail!("Workout name too long: {}", workout.name);
        }

        buf.push(name_bytes.len() as u8);
        buf.extend_from_slice(name_bytes);
        buf.push(workout.workout_type);

        buf.push(workout.exercises.len() as u8);
        for ex in workout.exercises {
            let ex_bytes = ex.as_bytes();
            if ex_bytes.len() > 255 {
                anyhow::bail!("Exercise name too long: {}", ex);
            }
            buf.push(ex_bytes.len() as u8);
            buf.extend_from_slice(ex_bytes);
        }
    }

    Ok(buf)
}

pub fn small_create_blank_replay(json_bytes: &[u8]) -> Result<(), ReplayCreationError> {
    use std::fs::write;

    let dummy_state = RecordingState {
        stage: slp_parser::Stage::FinalDestination,
        time: tm_replay::RecordingTime::today_approx(),
        filename: {
            let mut name = [0u8; 31];
            let base = b"workout_save_data";
            name[..base.len()].copy_from_slice(base);
            name
        },
        menu_settings: RecordingMenuSettings {
            hmn_mode: HmnRecordingMode::Playback,
            hmn_slot: RecordingSlot::Slot1,
            cpu_mode: CpuRecordingMode::Playback,
            cpu_slot: RecordingSlot::Slot1,
            ..Default::default()
        },
        start_frame: 0,
        hmn_state: CharacterState::default(), // Peach on FD in Wait state
        hmn_follower_state: None,
        cpu_state: CharacterState::default(),
        cpu_follower_state: None,
    };

    let custom_input = Input {
        button_flags: 0,
        stick_x: 0,
        stick_y: 0,
        cstick_x: 0,
        cstick_y: 0,
        trigger: 0,
    };

    // Store in a 1-frame input slice
    let input_frame = [custom_input];

    // Construct InputRecordings with only Slot 0 populated
    let inputs = InputRecordings {
        hmn_slots: [Some(&input_frame), None, None, None, None, None],
        cpu_slots: [None; 6],
    };

    let flags = ReplayFlags::default();

    let gci_bytes = funstruct_tm_replay(json_bytes, &dummy_state, &inputs, flags)?;
    write("workout_save_data.gci", gci_bytes)
        .expect("Failed to write output file");
    println!("Minimal GCI written to workout_save_data.gci");

    Ok(())
}



fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <json_file>", args[0]);
        std::process::exit(1);
    }

    let json_path = &args[1];
    //let json_data = fs::read(json_path)?;

    let encoded_data = encode_workouts_to_bytes(json_path)?;

    small_create_blank_replay(&encoded_data)
        .expect("Failed to create minimal GCI replay");
    
      Ok(())
    // Call your replay builde
}