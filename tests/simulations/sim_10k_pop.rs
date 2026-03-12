use crate::economy_sim_helper::{run_simulation, SimConfig};

#[test]
fn sim_10k_pop() {
    run_simulation(SimConfig {
        ticks: 200,
        pop: 10_000.0,
        show_macro_table: true,
        show_production: true,
        show_prices: true,
        show_labor: true,
        show_starved: true,
        show_capital: true,
        macro_tick_modulo: 10,
        ..Default::default()
    });
}
