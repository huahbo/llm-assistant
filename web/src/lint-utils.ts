export const normalizeLintSeverity = (severity: string) => {
  const normalized = severity.trim().toLowerCase();
  return normalized || "info";
};

export const formatLintCheckedAt = (checkedAt: string) => {
  const timestamp = Number(checkedAt);

  if (!Number.isFinite(timestamp)) {
    return checkedAt;
  }

  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return checkedAt;
  }

  return `${date.toISOString().slice(0, 19).replace("T", " ")} UTC`;
};
