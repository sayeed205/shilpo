import { assertEquals, assertThrows } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import { createTestHost } from "../src/testing/fake_host.ts";
import { DataValue } from "../src/data.ts";
import { HostError } from "../src/host.ts";

describe("Secrets Isolation & Policy", () => {
  it("state store strictly rejects storing secret-ref values", () => {
    const { facade } = createTestHost();
    const secretDv = DataValue.secretRef("sensitive-key-handle");

    assertThrows(
      () => {
        facade.state.write("auth_token", secretDv);
      },
      HostError,
      "SecretRef values cannot be stored in extension state",
    );
  });

  it("secret references and values never leak into snapshots or string output", () => {
    const { facade } = createTestHost();
    const secretBytes = new Uint8Array([1, 2, 3, 4, 5]);
    const secRef = facade.secrets.set("oauth-token", secretBytes);

    // Assert handle format
    assertEquals(secRef.handle.startsWith("sec-"), true);

    // Stringify check
    const json = JSON.stringify(secRef);
    assertEquals(json, `{"handle":"${secRef.handle}"}`);
    assertEquals(json.includes("1,2,3,4,5"), false);
  });
});
