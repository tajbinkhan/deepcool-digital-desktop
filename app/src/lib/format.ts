// Number formatting shared by the pages.

const GIB = 1024 ** 3;

export function toUnit(celsius: number, fahrenheit: boolean): number {
  return fahrenheit ? celsius * 1.8 + 32 : celsius;
}

export function unitSymbol(fahrenheit: boolean): string {
  return fahrenheit ? "℉" : "℃";
}

/** "43 ℃", or "--" without a sensor. */
export function temp(celsius: number | null | undefined, fahrenheit: boolean): string {
  if (celsius == null) return "--";
  return `${Math.round(toUnit(celsius, fahrenheit))} ${unitSymbol(fahrenheit)}`;
}

/** Whole gibibytes, as the reference shows them ("8/31 GB"). */
export function gb(bytes: number): number {
  return Math.round(bytes / GIB);
}

/** Decimal gigabytes for disks ("660 / 932 GB"). */
export function diskGb(bytes: number): number {
  return Math.round(bytes / 1e9);
}

export function percent(part: number, whole: number): number {
  return whole > 0 ? Math.round((part / whole) * 100) : 0;
}

/** One decimal below 10, whole numbers above: "1.1", "14", "0". */
function short(value: number): string {
  if (value === 0) return "0";
  return value < 10 ? (Math.round(value * 10) / 10).toString() : Math.round(value).toString();
}

/** Disk throughput, always in MB/s. */
export function diskRate(bytesPerSec: number): [string, string] {
  return [short(bytesPerSec / 1e6), "MB/s"];
}

/** Network throughput in KB/s, or MB/s from 1000 KB/s. */
export function netRate(bytesPerSec: number): [string, string] {
  const kb = bytesPerSec / 1e3;
  return kb >= 1000 ? [short(kb / 1e3), "MB/s"] : [Math.round(kb).toString(), "KB/s"];
}
