// IMPORTANT: All coefficients and prices in this file are illustrative synthetic data.
// Replace them with traceable supplier/laboratory data before engineering or commercial use.

export const sampleCatalog = {
  metadata: {
    id: 'illustrative-catalog-v0.1',
    revision: '2026-08-13',
    status: 'SYNTHETIC / NOT VENDOR DATA',
    currency: 'USD'
  },
  waterQualityFactors: {
    clean: { thermalMultiplier: 1.0, pressureMultiplier: 1.0, riskPenalty: 0 },
    moderate: { thermalMultiplier: 0.92, pressureMultiplier: 1.12, riskPenalty: 12000 },
    dirty: { thermalMultiplier: 0.80, pressureMultiplier: 1.35, riskPenalty: 30000 }
  },
  towers: [
    {
      id: 'IDCF-040',
      name: 'Illustrative 40 m² Induced-Draft Counterflow Cell',
      type: 'counterflow',
      draftType: 'induced',
      fillAreaM2: 40,
      airFreeAreaM2: 40,
      driftAreaM2: 38,
      inletAreaM2: 22,
      stackRecoveryFactor: 0.35,
      fillDepthOptionsM: [1.2, 1.5, 1.8],
      maxWaterMassFlowKgS: 155,
      footprintM2: 52,
      inletLossCoefficient: 3.2,
      distributionLossCoefficient: 4.6,
      supportLossCoefficient: 2.2,
      plenumLossCoefficient: 0.4,
      fixedPressureLossPa: 10,
      sprayZoneHeightM: 0.6,
      sprayZone: {
        coefficientPerM: 0.16,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.30,
        airExponent: 0.45
      },
      rainZoneHeightM: 1.4,
      rainZone: {
        coefficientPerM: 0.13,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.35,
        airExponent: 0.50
      },
      compatibleFanIds: ['AX-420', 'AX-500'],
      baseCost: 118000
    },
    {
      id: 'IDCF-064',
      name: 'Illustrative 64 m² Induced-Draft Counterflow Cell',
      type: 'counterflow',
      draftType: 'induced',
      fillAreaM2: 64,
      airFreeAreaM2: 64,
      driftAreaM2: 61,
      inletAreaM2: 32,
      stackRecoveryFactor: 0.35,
      fillDepthOptionsM: [1.2, 1.5, 1.8, 2.1],
      maxWaterMassFlowKgS: 265,
      footprintM2: 78,
      inletLossCoefficient: 3,
      distributionLossCoefficient: 4.4,
      supportLossCoefficient: 2.1,
      plenumLossCoefficient: 0.38,
      fixedPressureLossPa: 10,
      sprayZoneHeightM: 0.6,
      sprayZone: {
        coefficientPerM: 0.16,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.30,
        airExponent: 0.45
      },
      rainZoneHeightM: 1.5,
      rainZone: {
        coefficientPerM: 0.13,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.35,
        airExponent: 0.50
      },
      compatibleFanIds: ['AX-500', 'AX-600'],
      baseCost: 166000
    },
    {
      id: 'IDCF-096',
      name: 'Illustrative 96 m² Induced-Draft Counterflow Cell',
      type: 'counterflow',
      draftType: 'induced',
      fillAreaM2: 96,
      airFreeAreaM2: 96,
      driftAreaM2: 92,
      inletAreaM2: 43,
      stackRecoveryFactor: 0.35,
      fillDepthOptionsM: [1.2, 1.5, 1.8, 2.1],
      maxWaterMassFlowKgS: 410,
      footprintM2: 113,
      inletLossCoefficient: 2.8,
      distributionLossCoefficient: 4.2,
      supportLossCoefficient: 2,
      plenumLossCoefficient: 0.36,
      fixedPressureLossPa: 10,
      sprayZoneHeightM: 0.7,
      sprayZone: {
        coefficientPerM: 0.16,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.30,
        airExponent: 0.45
      },
      rainZoneHeightM: 1.6,
      rainZone: {
        coefficientPerM: 0.13,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.35,
        airExponent: 0.50
      },
      compatibleFanIds: ['AX-600', 'AX-700'],
      baseCost: 232000
    },
    {
      id: 'IDXF-080',
      name: 'Illustrative 80 m² Induced-Draft Crossflow Cell',
      type: 'crossflow',
      draftType: 'induced',
      fillAreaM2: 80,
      airFreeAreaM2: 72,
      driftAreaM2: 76,
      inletAreaM2: 88,
      stackRecoveryFactor: 0.25,
      fillDepthOptionsM: [1.2, 1.5, 1.8],
      maxWaterMassFlowKgS: 310,
      footprintM2: 118,
      inletLossCoefficient: 1.2,
      distributionLossCoefficient: 2.6,
      supportLossCoefficient: 1.6,
      plenumLossCoefficient: 0.34,
      fixedPressureLossPa: 8,
      sprayZoneHeightM: 0.5,
      sprayZone: {
        coefficientPerM: 0.16,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.30,
        airExponent: 0.45
      },
      rainZoneHeightM: 0.9,
      rainZone: {
        coefficientPerM: 0.13,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.35,
        airExponent: 0.50
      },
      compatibleFanIds: ['AX-500', 'AX-600'],
      baseCost: 205000
    }
  ],
  fills: [
    {
      id: 'FILM-CF19',
      name: 'Illustrative 19 mm Cross-Fluted Film Fill',
      geometry: 'cross-fluted film',
      compatibleTowerTypes: ['counterflow'],
      allowedWaterQualityClasses: ['clean'],
      thermal: {
        coefficientPerM: 1.28,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.34,
        airExponent: 0.46
      },
      pressure: {
        coefficientPaPerM: 78,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: 0.18,
        airExponent: 1.78
      },
      limits: {
        minWaterLoadingKgM2S: 1.5,
        maxWaterLoadingKgM2S: 5.0,
        minDryAirLoadingKgM2S: 1.0,
        maxDryAirLoadingKgM2S: 3.2,
        maxWaterTemperatureC: 55
      },
      material: 'PVC (illustrative)',
      costPerM3: 420
    },
    {
      id: 'FILM-OF25',
      name: 'Illustrative 25 mm Offset-Fluted Film Fill',
      geometry: 'offset-fluted film',
      compatibleTowerTypes: ['counterflow', 'crossflow'],
      allowedWaterQualityClasses: ['clean', 'moderate'],
      thermal: {
        coefficientPerM: 1.12,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.31,
        airExponent: 0.44
      },
      pressure: {
        coefficientPaPerM: 61,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: 0.14,
        airExponent: 1.72
      },
      limits: {
        minWaterLoadingKgM2S: 1.3,
        maxWaterLoadingKgM2S: 5.4,
        minDryAirLoadingKgM2S: 0.9,
        maxDryAirLoadingKgM2S: 3.3,
        maxWaterTemperatureC: 60
      },
      material: 'PP (illustrative)',
      costPerM3: 470
    },
    {
      id: 'FILM-VF38',
      name: 'Illustrative 38 mm Vertical-Fluted Fill',
      geometry: 'vertical-fluted film',
      compatibleTowerTypes: ['counterflow', 'crossflow'],
      allowedWaterQualityClasses: ['clean', 'moderate', 'dirty'],
      thermal: {
        coefficientPerM: 0.94,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.27,
        airExponent: 0.39
      },
      pressure: {
        coefficientPaPerM: 43,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: 0.10,
        airExponent: 1.67
      },
      limits: {
        minWaterLoadingKgM2S: 1.1,
        maxWaterLoadingKgM2S: 6.2,
        minDryAirLoadingKgM2S: 0.8,
        maxDryAirLoadingKgM2S: 3.5,
        maxWaterTemperatureC: 75
      },
      material: 'PP (illustrative)',
      costPerM3: 505
    },
    {
      id: 'TRICKLE-50',
      name: 'Illustrative 50 mm Trickle Fill',
      geometry: 'trickle grid',
      compatibleTowerTypes: ['counterflow', 'crossflow'],
      allowedWaterQualityClasses: ['moderate', 'dirty'],
      thermal: {
        coefficientPerM: 0.78,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.22,
        airExponent: 0.34
      },
      pressure: {
        coefficientPaPerM: 31,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: 0.08,
        airExponent: 1.58
      },
      limits: {
        minWaterLoadingKgM2S: 0.9,
        maxWaterLoadingKgM2S: 7.0,
        minDryAirLoadingKgM2S: 0.7,
        maxDryAirLoadingKgM2S: 3.7,
        maxWaterTemperatureC: 85
      },
      material: 'PP (illustrative)',
      costPerM3: 555
    },
    {
      id: 'SPLASH-GRID',
      name: 'Illustrative Open Splash Grid',
      geometry: 'splash',
      compatibleTowerTypes: ['counterflow', 'crossflow'],
      allowedWaterQualityClasses: ['moderate', 'dirty'],
      thermal: {
        coefficientPerM: 0.62,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: -0.18,
        airExponent: 0.31
      },
      pressure: {
        coefficientPaPerM: 22,
        referenceWaterLoadingKgM2S: 3.0,
        referenceDryAirLoadingKgM2S: 2.0,
        waterExponent: 0.06,
        airExponent: 1.52
      },
      limits: {
        minWaterLoadingKgM2S: 0.7,
        maxWaterLoadingKgM2S: 8.0,
        minDryAirLoadingKgM2S: 0.6,
        maxDryAirLoadingKgM2S: 4.0,
        maxWaterTemperatureC: 90
      },
      material: 'PP/FRP (illustrative)',
      costPerM3: 610
    }
  ],
  driftEliminators: [
    {
      id: 'DE-2P-LP',
      name: 'Illustrative Two-Pass Low-Pressure Eliminator',
      material: 'PVC (illustrative)',
      maxWaterTemperatureC: 60,
      curve: [
        { faceVelocityMS: 1.0, driftPpm: 35, pressureDropPa: 7 },
        { faceVelocityMS: 1.5, driftPpm: 45, pressureDropPa: 13 },
        { faceVelocityMS: 2.0, driftPpm: 62, pressureDropPa: 23 },
        { faceVelocityMS: 2.5, driftPpm: 88, pressureDropPa: 37 },
        { faceVelocityMS: 3.0, driftPpm: 130, pressureDropPa: 55 },
        { faceVelocityMS: 3.5, driftPpm: 200, pressureDropPa: 78 }
      ],
      costPerM2: 58
    },
    {
      id: 'DE-3P-10',
      name: 'Illustrative Three-Pass 0.001% Eliminator',
      material: 'PVC (illustrative)',
      maxWaterTemperatureC: 60,
      curve: [
        { faceVelocityMS: 1.0, driftPpm: 4, pressureDropPa: 10 },
        { faceVelocityMS: 1.5, driftPpm: 6, pressureDropPa: 18 },
        { faceVelocityMS: 2.0, driftPpm: 9, pressureDropPa: 31 },
        { faceVelocityMS: 2.5, driftPpm: 14, pressureDropPa: 49 },
        { faceVelocityMS: 3.0, driftPpm: 23, pressureDropPa: 72 },
        { faceVelocityMS: 3.5, driftPpm: 38, pressureDropPa: 101 }
      ],
      costPerM2: 78
    },
    {
      id: 'DE-4P-ULTRA',
      name: 'Illustrative Four-Pass Ultra-Low Drift Eliminator',
      material: 'PP (illustrative)',
      maxWaterTemperatureC: 80,
      curve: [
        { faceVelocityMS: 1.0, driftPpm: 1.2, pressureDropPa: 15 },
        { faceVelocityMS: 1.5, driftPpm: 1.8, pressureDropPa: 27 },
        { faceVelocityMS: 2.0, driftPpm: 2.8, pressureDropPa: 45 },
        { faceVelocityMS: 2.5, driftPpm: 4.8, pressureDropPa: 69 },
        { faceVelocityMS: 3.0, driftPpm: 8.5, pressureDropPa: 100 },
        { faceVelocityMS: 3.5, driftPpm: 15, pressureDropPa: 138 }
      ],
      costPerM2: 108
    }
  ],
  fans: [
    {
      id: 'AX-420',
      name: 'Illustrative 4.2 m Axial Fan',
      stackAreaM2: 13.854,
      pressureBasis: 'total',
      referenceDensityKgM3: 1.2,
      allowedSpeedRatio: [0.72, 1.12],
      driveEfficiency: 0.95,
      motorEfficiency: 0.94,
      cost: 24500,
      curve: [
        { flowM3S: 35, pressurePa: 390, efficiency: 0.61 },
        { flowM3S: 65, pressurePa: 350, efficiency: 0.72 },
        { flowM3S: 95, pressurePa: 275, efficiency: 0.81 },
        { flowM3S: 120, pressurePa: 185, efficiency: 0.80 },
        { flowM3S: 145, pressurePa: 65, efficiency: 0.66 }
      ]
    },
    {
      id: 'AX-500',
      name: 'Illustrative 5.0 m Axial Fan',
      stackAreaM2: 19.635,
      pressureBasis: 'total',
      referenceDensityKgM3: 1.2,
      allowedSpeedRatio: [0.70, 1.13],
      driveEfficiency: 0.96,
      motorEfficiency: 0.95,
      cost: 32200,
      curve: [
        { flowM3S: 55, pressurePa: 520, efficiency: 0.63 },
        { flowM3S: 100, pressurePa: 470, efficiency: 0.74 },
        { flowM3S: 145, pressurePa: 380, efficiency: 0.83 },
        { flowM3S: 185, pressurePa: 245, efficiency: 0.82 },
        { flowM3S: 225, pressurePa: 70, efficiency: 0.67 }
      ]
    },
    {
      id: 'AX-600',
      name: 'Illustrative 6.0 m Axial Fan',
      stackAreaM2: 28.274,
      pressureBasis: 'total',
      referenceDensityKgM3: 1.2,
      allowedSpeedRatio: [0.68, 1.14],
      driveEfficiency: 0.97,
      motorEfficiency: 0.955,
      cost: 41800,
      curve: [
        { flowM3S: 85, pressurePa: 650, efficiency: 0.65 },
        { flowM3S: 145, pressurePa: 585, efficiency: 0.76 },
        { flowM3S: 210, pressurePa: 465, efficiency: 0.85 },
        { flowM3S: 270, pressurePa: 300, efficiency: 0.84 },
        { flowM3S: 325, pressurePa: 85, efficiency: 0.69 }
      ]
    },
    {
      id: 'AX-700',
      name: 'Illustrative 7.0 m Axial Fan',
      stackAreaM2: 38.485,
      pressureBasis: 'total',
      referenceDensityKgM3: 1.2,
      allowedSpeedRatio: [0.66, 1.15],
      driveEfficiency: 0.97,
      motorEfficiency: 0.96,
      cost: 53500,
      curve: [
        { flowM3S: 120, pressurePa: 760, efficiency: 0.66 },
        { flowM3S: 205, pressurePa: 680, efficiency: 0.78 },
        { flowM3S: 295, pressurePa: 535, efficiency: 0.86 },
        { flowM3S: 375, pressurePa: 340, efficiency: 0.85 },
        { flowM3S: 450, pressurePa: 95, efficiency: 0.70 }
      ]
    }
  ],
  nozzles: [
    { id: 'NZ-20', name: 'Illustrative 20 mm Full-Cone Nozzle', dischargeCoefficient: 0.72, orificeDiameterM: 0.020 },
    { id: 'NZ-25', name: 'Illustrative 25 mm Full-Cone Nozzle', dischargeCoefficient: 0.74, orificeDiameterM: 0.025 },
    { id: 'NZ-32', name: 'Illustrative 32 mm Full-Cone Nozzle', dischargeCoefficient: 0.76, orificeDiameterM: 0.032 },
    { id: 'NZ-40', name: 'Illustrative 40 mm Full-Cone Nozzle', dischargeCoefficient: 0.77, orificeDiameterM: 0.040 }
  ]
};
