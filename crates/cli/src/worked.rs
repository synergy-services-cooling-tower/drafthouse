use crate::json::json_array;
use crate::json::object;
use crate::json::optional_number;
use crate::json::{fixed, number};
use crate::json_string;
use drafthouse_engine::{SelectionCandidate, SelectionRun};

pub(crate) const FILL_LAYER_STEP_POSITION: usize = 4;

pub(crate) fn fill_layer_steps(candidate: &SelectionCandidate) -> Vec<String> {
    let count = candidate.fill_layers.len();
    candidate
        .fill_layers
        .iter()
        .map(|layer| {
            worked_step(
                &format!(
                    "Fill layer {} of {} — {} @ {} m",
                    layer.position,
                    count,
                    layer.fill_id,
                    fixed(layer.depth_m, 4)
                ),
                "One layer of the stack, in physical order. The layers sit in series in the air \
                 path and in the water path, so every layer is evaluated at the same water and \
                 dry-air loadings; this layer contributes its own fill characteristic over its \
                 own depth.",
                "KaV/L_layer = c * (L''/L''ref)^a * (G''/G''ref)^b * depth * multiplier",
                &format!(
                    "L'' {} vs {}, G'' {} vs {}, depth {} m, multiplier {}",
                    fixed(layer.water_loading_kg_m2_s, 4),
                    fixed(layer.thermal.reference_water_loading_kg_m2_s, 4),
                    fixed(layer.dry_air_loading_kg_m2_s, 4),
                    fixed(layer.thermal.reference_dry_air_loading_kg_m2_s, 4),
                    fixed(layer.depth_m, 4),
                    fixed(layer.effective_thermal_multiplier, 4)
                ),
                number(layer.merkel_number),
                "-",
                Some("Merkel (1925); ASHRAE Systems and Equipment, Cooling Towers"),
            )
        })
        .collect()
}

pub(crate) fn selection_worked_object(
    run: &SelectionRun,
    candidate: &SelectionCandidate,
) -> String {
    let mut steps = duty_steps(run, candidate);
    steps.extend(air_steps(run, candidate));
    steps.extend(demand_steps(candidate));
    steps.extend(supply_steps(run, candidate));
    steps.extend(power_steps(candidate));
    steps.extend(water_steps(run, candidate));
    steps.extend(closing_notes());
    steps.splice(
        FILL_LAYER_STEP_POSITION..FILL_LAYER_STEP_POSITION,
        fill_layer_steps(candidate),
    );
    let steps = json_array(&steps);

    object(&[
        ("title", json_string("Selection of the recommended unit")),
        (
            "purpose",
            json_string(
                "One duty in, one ranked unit out: the numbers the recommendation rests on, \
                 in the order they were computed.",
            ),
        ),
        ("steps", steps),
        ("result", worked_result(candidate)),
    ])
}

fn duty_steps(run: &SelectionRun, candidate: &SelectionCandidate) -> Vec<String> {
    let requirements = &run.requirements;
    let thermal = &candidate.thermal;
    let range_c = requirements.hot_water_c - requirements.target_cold_water_c;
    vec![
        worked_step(
            "Duty water flow",
            "The flow the unit has to hold at the required cold-water temperature. It is a \
         constraint on every candidate, never a sort key.",
            "Q_w = duty flow",
            &format!(
                "{} kg/s (given)",
                fixed(requirements.water_mass_flow_kg_s, 4)
            ),
            number(requirements.water_mass_flow_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Range",
            "How far the water is cooled, which fixes the heat it gives up per kilogram.",
            "range = t_hot - t_cold,target",
            &format!(
                "{} - {}",
                fixed(requirements.hot_water_c, 4),
                fixed(requirements.target_cold_water_c, 4)
            ),
            number(range_c),
            "K",
            None,
        ),
        worked_step(
            "Heat the water gives up",
            "The water-side duty the air side has to match, at the water's own heat capacity \
         at the mean temperature.",
            "Q = Q_w * c_p,w * range",
            &format!(
                "{} * {} * {}",
                fixed(requirements.water_mass_flow_kg_s, 4),
                fixed(thermal.cp_water_kj_kg_k, 4),
                fixed(range_c, 4)
            ),
            number(thermal.heat_transfer_kw),
            "kW",
            None,
        ),
    ]
}

fn air_steps(run: &SelectionRun, candidate: &SelectionCandidate) -> Vec<String> {
    let inlet = &run.inlet_air_state;
    let airside = &candidate.airside;
    vec![
        worked_step(
            "Dry-air mass flow",
            "The air the fill actually passes at this duty; the air-side quantities below are \
         all set by it.",
            "G = rho_da * A_fill * v_fill",
            &format!(
                "{} * {} * {}",
                fixed(inlet.dry_air_density_kg_m3, 4),
                fixed(airside.areas.fill_area_m2, 4),
                fixed(airside.fill_velocity_ms, 4)
            ),
            number(airside.dry_air_mass_flow_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Fill transfer demand",
            "The transfer number the fill alone has to deliver over the range, integrated \
         against the air's saturation line.",
            "KaV/L_fill = integral c_p,w dt / (h_s(t) - h_a)",
            "evaluated over the fill height at the solved water path",
            number(airside.fill_merkel_number),
            "-",
            Some("Merkel (1925); ASHRAE Systems and Equipment, Cooling Towers"),
        ),
    ]
}

fn demand_steps(candidate: &SelectionCandidate) -> Vec<String> {
    let airside = &candidate.airside;
    let total_merkel = airside.fill_merkel_number
        + airside.spray_zone_merkel_number
        + airside.rain_zone_merkel_number;
    vec![
        worked_step(
            "Spray and rain zone demand",
            "The zones above and below the fill transfer heat too, on the same correlation \
         form with their own loadings.",
            "KaV/L_zones = KaV/L_spray + KaV/L_rain",
            &format!(
                "{} + {}",
                fixed(airside.spray_zone_merkel_number, 4),
                fixed(airside.rain_zone_merkel_number, 4)
            ),
            number(airside.spray_zone_merkel_number + airside.rain_zone_merkel_number),
            "-",
            None,
        ),
        worked_step(
            "Total demand",
            "Fill plus zones: what the tower must supply to cool the water to the solved \
         cold-water temperature.",
            "KaV/L_req = KaV/L_fill + KaV/L_spray + KaV/L_rain",
            &format!(
                "{} + {} + {}",
                fixed(airside.fill_merkel_number, 4),
                fixed(airside.spray_zone_merkel_number, 4),
                fixed(airside.rain_zone_merkel_number, 4)
            ),
            number(total_merkel),
            "-",
            None,
        ),
    ]
}

fn supply_steps(run: &SelectionRun, candidate: &SelectionCandidate) -> Vec<String> {
    let requirements = &run.requirements;
    let thermal = &candidate.thermal;
    let airside = &candidate.airside;
    vec![
        worked_step(
            "Available transfer",
            "What this fill at this depth, this water loading and this speed ratio can supply.",
            "KaV/L_avail = C * (L/G)^m",
            &format!(
                "at L/G = {}, fill depth {} m",
                fixed(
                    requirements.water_mass_flow_kg_s / airside.dry_air_mass_flow_kg_s,
                    4
                ),
                fixed(candidate.fill_depth_m, 4)
            ),
            number(airside.available_merkel_number),
            "-",
            None,
        ),
        worked_step(
            "Capability ratio",
            "Available over required at the duty, before the margin is taken: above 1 the \
         unit has headroom, below 1 it misses the target.",
            "capability = KaV/L_avail,duty / KaV/L_req,duty",
            "solved at the target cold-water temperature, not at the solved one",
            optional_number(candidate.capability_ratio),
            "-",
            None,
        ),
        worked_step(
            "Cold-water temperature, solved",
            "The temperature the tower actually reaches: the available transfer is matched \
         against the demand by root solve.",
            "solve KaV/L_avail(t) = KaV/L_req(t)",
            &format!(
                "target {}, margin {}",
                fixed(requirements.target_cold_water_c, 4),
                fixed(candidate.thermal_margin_c, 4)
            ),
            number(thermal.cold_water_c),
            "degC",
            None,
        ),
    ]
}

fn power_steps(candidate: &SelectionCandidate) -> Vec<String> {
    let airside = &candidate.airside;
    let fan = &candidate.fan_operating_point;
    vec![
        worked_step(
            "Air-side pressure demand",
            "Every loss the fan has to overcome, summed over the path.",
            "dp = fill + drift + inlet + distribution + support + plenum + fan stack + fixed",
            &format!(
                "{} + {} + {} + {} + {} + {} + {} + {}",
                fixed(airside.fill_pa, 4),
                fixed(airside.drift_pa, 4),
                fixed(airside.inlet_pa, 4),
                fixed(airside.distribution_pa, 4),
                fixed(airside.support_pa, 4),
                fixed(airside.plenum_pa, 4),
                fixed(airside.fan_stack_pa, 4),
                fixed(airside.fixed_pa, 4)
            ),
            number(airside.total_pa),
            "Pa",
            None,
        ),
        worked_step(
            "Fan operating point",
            "Where the fan curve and the system curve cross; the flow and pressure the fan \
         actually delivers.",
            "dp_fan(Q) = dp_system(Q)",
            &format!(
                "fan {} Pa vs system {} Pa at {} m3/s",
                fixed(fan.fan_pressure_pa, 4),
                fixed(fan.system_pressure_pa, 4),
                fixed(fan.flow_m3_s, 4)
            ),
            number(fan.flow_m3_s),
            "m3/s",
            None,
        ),
        worked_step(
            "Fan shaft power",
            "The shaft power at that operating point and the fan's efficiency there.",
            "P_shaft = Q * dp_fan / eta_fan",
            &format!(
                "{} * {} / {}",
                fixed(fan.flow_m3_s, 4),
                fixed(fan.fan_pressure_pa, 4),
                fixed(fan.efficiency, 4)
            ),
            number(fan.shaft_power_kw),
            "kW",
            Some("AMCA 210 fan total vs static pressure definitions"),
        ),
    ]
}

fn water_steps(run: &SelectionRun, candidate: &SelectionCandidate) -> Vec<String> {
    let requirements = &run.requirements;
    let inlet = &run.inlet_air_state;
    let thermal = &candidate.thermal;
    let airside = &candidate.airside;
    let balance = &candidate.water_balance;
    vec![
        worked_step(
            "Evaporation",
            "Water the air carries away, from the dry-air mass balance between inlet and \
         leaving humidity ratio.",
            "E = G * (W_out - W_in)",
            &format!(
                "{} * ({} - {})",
                fixed(airside.dry_air_mass_flow_kg_s, 4),
                fixed(thermal.outlet_air_state.humidity_ratio, 6),
                fixed(inlet.humidity_ratio, 6)
            ),
            number(balance.evaporation_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Make-up water",
            "Evaporation concentrated by the cycles of concentration, plus the drift that \
         leaves with the air.",
            "M = E * N/(N-1) + D",
            &format!(
                "{} * {}/({} - 1) + {}",
                fixed(balance.evaporation_kg_s, 4),
                fixed(requirements.cycles_of_concentration, 4),
                fixed(requirements.cycles_of_concentration, 4),
                fixed(balance.drift_kg_s, 6)
            ),
            number(balance.makeup_kg_s),
            "kg/s",
            None,
        ),
        worked_step(
            "Capacity",
            "The flow the unit holds at the required cold-water temperature, re-solving the \
         air side at each flow: the ranking metric of the default objective.",
            "Q_w,max = root on Q_w at t_cold(Q_w) = target",
            "bracketed root on the water mass flow",
            optional_number(candidate.capacity_kg_s),
            "kg/s",
            None,
        ),
    ]
}

fn closing_notes() -> Vec<String> {
    vec![
        worked_note(
            "Ranking",
            "The candidate above is rank 1 under the run's declared objective. Duty flow and \
         the thermal margin constrain the feasible set; they are never sort keys.",
            None,
        ),
        worked_note(
            "Where these numbers come from",
            "Every value in this sheet is read from the selection result itself - the same \
         solve that ranked the candidate - so the sheet cannot drift from the engine.",
            None,
        ),
    ]
}

fn worked_result(candidate: &SelectionCandidate) -> String {
    let thermal = &candidate.thermal;
    let airside = &candidate.airside;
    let fan = &candidate.fan_operating_point;
    let balance = &candidate.water_balance;
    object(&[
        ("rank", candidate.rank.to_string()),
        ("towerId", json_string(&candidate.tower_id)),
        ("fillId", json_string(&candidate.fill_id)),
        (
            "driftEliminatorId",
            json_string(&candidate.drift_eliminator_id),
        ),
        ("fanId", json_string(&candidate.fan_id)),
        ("nozzleId", json_string(&candidate.nozzle.nozzle_id)),
        ("fillDepthM", number(candidate.fill_depth_m)),
        ("speedRatio", number(candidate.speed_ratio)),
        ("coldWaterC", number(thermal.cold_water_c)),
        ("thermalMarginC", number(candidate.thermal_margin_c)),
        ("rangeC", number(thermal.range_c)),
        ("heatTransferKW", number(thermal.heat_transfer_kw)),
        ("electricalInputKW", number(candidate.electrical_input_kw)),
        (
            "motorRequiredKW",
            number(candidate.motor.required_motor_output_kw),
        ),
        (
            "motorSelectedKW",
            optional_number(candidate.motor.selected_motor_kw),
        ),
        ("capacityKgS", optional_number(candidate.capacity_kg_s)),
        (
            "capabilityRatio",
            optional_number(candidate.capability_ratio),
        ),
        ("totalPa", number(airside.total_pa)),
        ("fanFlowM3S", number(fan.flow_m3_s)),
        ("fanPressurePa", number(fan.fan_pressure_pa)),
        ("fanShaftPowerKW", number(fan.shaft_power_kw)),
        ("evaporationKgS", number(balance.evaporation_kg_s)),
        ("makeupKgS", number(balance.makeup_kg_s)),
    ])
}

pub(crate) fn worked_step(
    label: &str,
    why: &str,
    formula: &str,
    substitution: &str,
    value: String,
    unit: &str,
    reference: Option<&str>,
) -> String {
    object(&[
        ("label", json_string(label)),
        ("why", json_string(why)),
        ("formula", json_string(formula)),
        ("substitution", json_string(substitution)),
        ("value", value),
        ("unit", json_string(unit)),
        (
            "reference",
            reference.map_or_else(|| "null".to_string(), json_string),
        ),
        ("kind", json_string("calc")),
    ])
}

pub(crate) fn worked_note(label: &str, why: &str, reference: Option<&str>) -> String {
    object(&[
        ("label", json_string(label)),
        ("why", json_string(why)),
        ("formula", "null".to_string()),
        ("substitution", "null".to_string()),
        ("value", "null".to_string()),
        ("unit", json_string("")),
        (
            "reference",
            reference.map_or_else(|| "null".to_string(), json_string),
        ),
        ("kind", json_string("note")),
    ])
}
