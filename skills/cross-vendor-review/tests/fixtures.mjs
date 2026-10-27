export const verifiedSmallChange = {
  complexity: "low",
  risk: "low",
  rationale: "One local display-label change; no data, permissions, external contracts or shared behaviour changes.",
  checks: {
    requirements: { passed: true, evidence: "The packet names the expected label and every acceptance condition." },
    tests: { passed: true, evidence: "The focused display-label test passed; its command and output are in the packet." },
    coverage: { passed: true, evidence: "The existing test checks the normal and empty-value labels." },
    familiarity: { passed: true, evidence: "The implementation follows the adjacent display-label mapping without a new abstraction." },
  },
};
