import { assert, assertEquals } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import { dirname, fromFileUrl, resolve } from "@std/path";

const DIR = dirname(fromFileUrl(import.meta.url));
const ROOT_DIR = resolve(DIR, "../../../..");
const WIT_DIR = resolve(ROOT_DIR, "core/ext-api/wit");
const SOURCE_FILE = resolve(DIR, "extension.ts");
const WASM_FILE = resolve(DIR, "extension.wasm");

describe("Component Conformance Fixture", () => {
  it("builds TypeScript extension into valid Wasm component", async () => {
    // 1. Run jco componentize
    const cmd = new Deno.Command("npx", {
      args: [
        "--yes",
        "@bytecodealliance/jco@1",
        "componentize",
        SOURCE_FILE,
        "--wit",
        WIT_DIR,
        "--world-name",
        "extension",
        "-o",
        WASM_FILE,
      ],
      stdout: "piped",
      stderr: "piped",
    });

    const output = await cmd.output();
    const stderrText = new TextDecoder().decode(output.stderr);
    const stdoutText = new TextDecoder().decode(output.stdout);

    assert(
      output.success,
      `jco componentize failed:\nstdout: ${stdoutText}\nstderr: ${stderrText}`,
    );

    // 2. Validate output binary
    const wasmBytes = await Deno.readFile(WASM_FILE);
    assert(wasmBytes.length > 1000, "WASM binary must be non-empty");

    // Check WASM component magic header: 0x00 0x61 0x73 0x6d 0x0d 0x00 0x01 0x00
    assertEquals(wasmBytes[0], 0x00);
    assertEquals(wasmBytes[1], 0x61);
    assertEquals(wasmBytes[2], 0x73);
    assertEquals(wasmBytes[3], 0x6d);
    assertEquals(wasmBytes[4], 0x0d);
    assertEquals(wasmBytes[5], 0x00);
    assertEquals(wasmBytes[6], 0x01);
    assertEquals(wasmBytes[7], 0x00);

    console.log(`Successfully compiled and verified WASM component (${wasmBytes.length} bytes)`);
  });
});
