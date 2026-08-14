import { dirname, fromFileUrl, resolve } from "@std/path";

const DIR = dirname(fromFileUrl(import.meta.url));

async function run(cmd: string, args: string[]): Promise<void> {
  console.log(`Running: ${cmd} ${args.join(" ")}`);
  const process = new Deno.Command(cmd, {
    args,
    stdout: "inherit",
    stderr: "inherit",
  });
  const output = await process.output();
  if (!output.success) {
    throw new Error(`Command failed with code ${output.code}: ${cmd} ${args.join(" ")}`);
  }
}

async function main(): Promise<void> {
  console.log("=== Running Deno Consumer Smoke Test ===");
  await run("deno", ["run", resolve(DIR, "deno/main.ts")]);

  console.log("=== Running Bun Consumer Smoke Test ===");
  await run("bun", ["run", resolve(DIR, "bun/main.ts")]);

  console.log("=== Running Node.js Consumer Smoke Test ===");
  await run("node", ["--experimental-strip-types", resolve(DIR, "node/main.ts")]);

  console.log("All cross-runtime consumer smoke tests passed.");
}

if (import.meta.main) {
  await main();
}
