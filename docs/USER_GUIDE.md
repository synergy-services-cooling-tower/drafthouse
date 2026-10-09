# User Guide

## 1. Starting the application

From the project directory:

```bash
npm start
```

Use a modern browser to open the printed local address. The application stores no server-side data. Calculation exports are generated locally as JSON.

## 2. Merkel rating workflow

Use this page for a transparent thermal calculation at one operating condition.

### Required inputs

- circulating-water mass flow;
- dry-air mass flow;
- hot- and known cold-water temperatures;
- entering dry- and wet-bulb temperatures;
- atmospheric pressure;
- salinity, where applicable;
- an available whole-tower or fill-stack Merkel number; and
- a characteristic exponent for plotting the supply curve.

### Outputs

- water-to-dry-air ratio;
- entering humidity ratio, relative humidity, enthalpy, and density;
- range, approach, effectiveness, and heat rejection;
- required Merkel number using Simpson integration;
- four-point Tchebycheff comparison;
- predicted CWT for the specified available Merkel number;
- estimated saturated outlet-air state;
- evaporation, drift, blowdown, and makeup; and
- demand and available-characteristic curves.

### Interpretation

If the available Merkel number equals the required Merkel number, the predicted CWT should be close to the entered known CWT. A lower available Merkel number produces warmer water; a higher available Merkel number produces colder water.

A pinch error means the assumed air operating line reaches or exceeds saturated-air enthalpy at a local water temperature. This condition is thermodynamically infeasible under the model assumptions.

## 3. CTI-style characteristic capability workflow

Enter separate design and test conditions and the whole-tower exponent.

The prototype:

1. calculates the test `KaV/L`;
2. constructs `KaV/L = C(L/G)^m` through the test point;
3. generates the demand curve at the design thermal condition;
4. solves their intersection; and
5. reports the ratio of intersection `L/G` to design `L/G` as capability percentage.

The seeded Monte Carlo calculation perturbs the test measurements using the entered standard deviations. It reports the median, standard deviation, approximate expanded uncertainty, and central 95% interval.

### Contractual warning

Do not use this result as an ATC-105 acceptance result until the code has been validated against the licensed current standard, licensed CTI Toolkit, the contractual reference data, and the project-specific test procedure.

## 4. Tower and component selection workflow

Enter the required duty and hard constraints. The search evaluates every compatible synthetic combination.

### Hard constraints

- tower water-flow limit;
- tower footprint;
- fill type and water-quality class;
- fill air/water-loading envelope;
- fill and drift material temperature;
- drift face-velocity curve range;
- fan operating curve and speed range;
- predicted cold-water temperature and minimum margin;
- maximum drift;
- maximum electrical input; and
- availability of a standard motor and nozzle arrangement.

### Ranking

Feasible candidates are ranked by illustrative lifecycle cost:

```text
CAPEX + present value(energy + water + maintenance) + water-quality risk penalty
```

The top result includes the component set, thermal model, air and water loading, pressure-loss breakdown, fan efficiency, motor, nozzle count, water use, CAPEX, and annual energy.

### Practical selection review

Before approving a candidate, an engineer should additionally review:

- recirculation and site layout;
- plume, icing, noise, wind, and seismic requirements;
- distribution uniformity and minimum nozzle head;
- structural loads and fill support;
- access and replacement method;
- fire classification;
- chemical and temperature compatibility;
- hazardous-area requirements;
- vibration and fan mechanical limits;
- redundancy and cell operating modes; and
- contractual certification and testing requirements.

## 5. Performance-curve workflow

The bundled sample grid is rectangular in wet bulb, range, and water flow. The program interpolates CWT on the surrounding grid corners and solves the reference flow that matches the test CWT.

Capability is reported as:

```text
100 × adjusted test water flow / predicted reference water flow
```

The page also reports leaving-water deviation under the explicit sign convention that positive means warmer and worse than the reference prediction.

## 6. Nozzle utility

The nozzle utility converts water mass flow to volume flow, applies the orifice equation to each sample nozzle, and rounds up to the required count. It does not yet evaluate spray overlap, header hydraulics, nozzle elevation, distribution uniformity, or clogging risk.

## 7. Water-balance utility

The calculator uses:

```text
makeup = evaporation + drift + blowdown
blowdown = max(0, evaporation / (COC − 1) − drift)
```

The relation assumes dissolved solids enter through makeup and leave mainly through blowdown and drift. Real systems may require leakage, basin overflow, windage, treatment discharge, and process contamination terms.

## 8. Natural-draft demonstration

This solver iterates airflow until approximate stack buoyancy equals tower resistance. At each trial airflow it recalculates:

- dry-air flow and `L/G`;
- fill Merkel capability and wet pressure drop;
- predicted CWT;
- saturated outlet-air temperature and humidity;
- plume density;
- draft pressure; and
- total system resistance.

A production natural-draft implementation needs vertical and radial integration, shell geometry, rain-zone transfer, wind effects, multiple fill rings, nonuniform water distribution, and validated large-tower loss coefficients.

## 9. Export and print

After a successful calculation, use **Export last result** to save the full structured result as JSON. Browser print mode hides the forms and navigation so results can be printed or saved as PDF.
