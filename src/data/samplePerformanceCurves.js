const wetBulbs = [24, 27, 30];
const ranges = [8, 10, 12];
const flows = [120, 160, 200, 240, 280];

export const samplePerformanceCurveRecords = wetBulbs.flatMap((wetBulbC) =>
  ranges.flatMap((rangeC) =>
    flows.map((waterFlowKgS) => {
      const flowRatio = waterFlowKgS / 200;
      const approachC = 3.15 + 2.65 * flowRatio ** 1.45 + 0.11 * (rangeC - 10) + 0.012 * (wetBulbC - 27) ** 2;
      return {
        wetBulbC,
        rangeC,
        waterFlowKgS,
        coldWaterC: wetBulbC + approachC
      };
    })
  )
);

export const samplePerformanceCurveMetadata = {
  name: 'Synthetic reference performance-curve grid',
  status: 'ILLUSTRATIVE / NOT MANUFACTURER OR CTI CERTIFIED DATA',
  dimensions: {
    wetBulbC: wetBulbs,
    rangeC: ranges,
    waterFlowKgS: flows
  }
};
