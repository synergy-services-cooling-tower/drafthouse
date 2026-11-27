# References and Provenance

Public web sources were checked on 13 August 2026. Paid standards and commercial software must be obtained through their publishers and used under their licence terms.

## 1. Cooling Technology Institute

1. **Cooling Technology Institute — CTI Toolkit 4.3.** Public feature description covering ASHRAE-compliant psychrometrics, the Thermal Design Worksheet/demand curve, induced/forced and crossflow/counterflow Performance Evaluator, percent performance, leaving-water deviation and automatic crossplotting.  
   https://www.cti.org/toolkit

2. **Cooling Technology Institute Marketplace — Acceptance Test Code ATC-105.** Public scope description of test procedures/instrumentation, characteristic- and performance-curve evaluation, water cooling capacity, examples and `KaV/L`; revised September 2022. The paid publication is required for a controlled implementation.  
   https://cti-marketplace.myshopify.com/products/act-105

3. **Cooling Technology Institute Marketplace — Isokinetic Drift Measurement Test Code ATC-140.** Public purpose description for drift instrumentation, testing and evaluation; revised February 2023.  
   https://cti-marketplace.myshopify.com/products/atc-140

4. **Cooling Technology Institute — CTI Certification.** Public explanation that STD-201 certification verifies manufacturer-published performance through independent testing of a model or its model line.  
   https://www.cti.org/cti-certification

5. **Cooling Technology Institute Marketplace — STD-201 Operations Manual.** Public description of the operations manual governing compliance with the latest STD-201RS; current marketplace page lists October 2025.  
   https://cti-marketplace.myshopify.com/products/std-201om

## 2. MRL cooling-tower software

6. **Richard Aull Cooling Tower Consulting — MRL Cooling Tower Software.** Public descriptions of IDCF, IDXF, FDCF, NDCF, NDXF and CFPC; supplier fill-data inclusion; fresh/seawater scope; NDCF multiple fills and concentric rings; and the statement that source code is not provided.  
   https://www.raullctc.com/MRL-Cooling-Tower-Software

## 3. Psychrometrics and water properties

7. **PsychroLib — Overview.** Open-source multi-language psychrometric library, including JavaScript, based on the 2017 ASHRAE Handbook — Fundamentals, Chapter 1. Used as a transparent comparison reference; no PsychroLib code is bundled.  
   https://github.com/psychrometrics/psychrolib/blob/master/docs/overview.md

8. **ASHRAE — Fundamentals of Psychrometrics.** Public educational description of moist-air properties and psychrometric calculations. The current ASHRAE Handbook should govern production qualification.  
   https://www.ashrae.org/professional-development/self-directed-learning-group-learning-texts/fundamentals-of-psychrometrics

9. **International Association for the Properties of Water and Steam — Revised Release on IAPWS-IF97.** Official industrial formulation reference for ordinary-water and steam properties.  
   https://iapws.org/documents/release/IF97-Rev

10. **IAPWS — Releases, Supplementary Releases, Guidelines and Advisory Notes.** Official index of property formulations and current releases.  
    https://iapws.org/documents/release

## 4. Merkel, Poppe and cooling-tower modelling

11. **Kloppers, J. C. and Kröger, D. G. — “Cooling Tower Performance Evaluation: Merkel, Poppe, and e-NTU Methods of Analysis.”** *Journal of Engineering for Gas Turbines and Power*, 127(1), 2005, DOI 10.1115/1.1787504. Reference for the differences among Merkel, Poppe and e-NTU analyses.  
    https://doi.org/10.1115/1.1787504

12. **Kloppers, J. C. and Kröger, D. G. — “A Critical Investigation into the Heat and Mass Transfer Analysis of Crossflow Wet-Cooling Towers.”** *Numerical Heat Transfer, Part A*, 46(8), 2004. Reference for the need for geometry-specific crossflow treatment.  
    https://doi.org/10.1080/10407780490478578

13. **Picardo, J. R. and Variyar, J. E. — “The Merkel Equation Revisited: A Novel Method to Compute the Packed Height of a Cooling Tower.”** *Energy Conversion and Management*, 57, 2012, DOI 10.1016/j.enconman.2011.12.016.  
    https://doi.org/10.1016/j.enconman.2011.12.016

14. **SPX Cooling Technologies — “A Comprehensive Approach to the Analysis of Cooling Tower Performance.”** Public engineering overview of the Merkel enthalpy-potential concept. Manufacturer source; used as secondary explanatory material.  
    https://spxcooling.com/library/a-comprehensive-approach-to-the-analysis-of-cooling-tower-performance/

## 5. Fans and air systems

15. **Air Movement and Control Association International — Fan and Blower Applications Engineering.** Public course description identifying fan/system curves and affinity-law changes in airflow, pressure and power with speed/density.  
    https://learning.amca.org/store/5041043-fan-and-blower-applications-engineering-i-foundations-june-2026

16. **AMCA — “Straightening Out Fan Curves.”** Public guidance on duty points, pressure, power, density/speed information, motor overload and installation effects.  
    https://www.amca.org/educate/articles-and-technical-papers/amca-inmotion-articles/straightening-out-fan-curves.html

18. **Chart Industries (Hudson Products Corporation) — R. C. Monroe, “Fans Key to Optimum Cooling-Tower Design.”** Paper presented to the Cooling Technology Institute annual meeting, New Orleans, 1974. States the basic fan law for axial cooling-tower fans — `CFM = f(rpm)¹`, `TP = f(rpm)²`, `HP = f(rpm)³` — and the tip-speed practice U.S. designs are usually held to (12 000 ft/min standard; 10 000 ft/min in the worked example). Cited by `docs/CTI_MRL_MAPPING.md` for the affinity exponents and used for the rated-speed datum the fan records carry (`nominalRpm`); checked 23 September 2026.  
    https://files.chartindustries.com/hudson/Fans-Key-to-Cooling-Tower-Design.pdf

## 6. Cooling-tower water balance

17. **U.S. Department of Energy FEMP — Best Management Practice #10: Cooling Tower Management.** Public explanation of evaporation, drift, blowdown, leaks/overflow, makeup balance and cycles of concentration.  
    https://www.energy.gov/cmei/femp/best-management-practice-10-cooling-tower-management

## 7. Implementation provenance

- The repository does not reproduce paid CTI text, tables, examples or forms.
- The MRL mapping is limited to public product descriptions; exact mechanics remain unknown because source code and proprietary data are not supplied.
- Core equations were implemented independently in JavaScript.
- Sample component records and performance curves are synthetic and are not digitized from a supplier catalog.
- Every commercial data import should retain source, revision, units, validity envelope, uncertainty, licence and approval status.
