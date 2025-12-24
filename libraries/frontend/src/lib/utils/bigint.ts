export const Max = (...bigints: bigint[]) =>
  bigints.reduce((a, b) => (a > b ? a : b), 0n);

export const Min = (...bigints: bigint[]) =>
  bigints.reduce(
    (a, b) => (a !== undefined && a < b ? a : b) ?? 0n,
    undefined as bigint | undefined
  ) ?? 0n;

export const FloatToBigInt = <
  Param extends number | undefined = number | undefined,
  Return = Param extends number ? bigint : undefined,
>(
  f: Param,
  decimals: number
): Return => {
  if (f === undefined) return undefined as Return;
  if (f.toString().startsWith("1e-")) return 0n as Return;
  if (decimals === 0) return BigInt(f) as Return;
  // Convert float to string and move the dot back 8 places to avoid floating point errors
  const [whole, decimal = ""] = f.toFixed(decimals).split(".");

  // Decimals converted to string of 8 digits
  const decimalStr = decimal.slice(0, decimals).padEnd(decimals, "0");
  return BigInt(whole + decimalStr) as Return;
};

export const BigIntToFloat = <
  Param extends bigint | undefined = bigint | undefined,
  Return = Param extends bigint ? number : undefined,
>(
  amount: Param,
  decimals: number
): Return => {
  if (amount === undefined) return undefined as Return;
  if (decimals === 0) return parseFloat(amount.toString()) as Return;
  const str =
    amount.toString().slice(0, -decimals) +
    "." +
    amount.toString().slice(-decimals).padStart(decimals, "0");
  return parseFloat(str) as Return;
};

export const BigIntToString = (
  amount: bigint,
  decimals: number,
  renderedDecimalPlaces = 4
) => {
  if (decimals === 0) return amount.toString();
  const beforeDot = amount.toString().slice(0, -decimals);
  const afterDot = amount
    .toString()
    .slice(-decimals)
    .padStart(decimals, "0")
    .slice(0, renderedDecimalPlaces)
    .replace(/0+$/, "");

  if (!beforeDot && !afterDot) return "0";
  if (!beforeDot) return `0.${afterDot}`;
  if (!afterDot) return beforeDot;
  return `${beforeDot}.${afterDot}`;
};
