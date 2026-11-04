import { describe, expect, it } from "vitest";
import { matchesQuery, parseQuickConnect } from "./parse";

describe("parseQuickConnect", () => {
  it("reads user, host and port", () => {
    expect(parseQuickConnect("pi@10.20.30.41")).toEqual({ user: "pi", host: "10.20.30.41", port: 22 });
    expect(parseQuickConnect("root@build.example.com:2222")).toEqual({ user: "root", host: "build.example.com", port: 2222 });
    expect(parseQuickConnect("example.com")).toEqual({ user: "", host: "example.com", port: 22 });
    expect(parseQuickConnect("server:2200")).toEqual({ user: "", host: "server", port: 2200 });
  });

  it("does not claim ordinary words or bad ports", () => {
    expect(parseQuickConnect("split")).toBeNull();
    expect(parseQuickConnect("new tab")).toBeNull();
    expect(parseQuickConnect("host:0")).toBeNull();
    expect(parseQuickConnect("host:70000")).toBeNull();
    expect(parseQuickConnect("a b@c")).toBeNull();
    expect(parseQuickConnect("")).toBeNull();
  });
});

describe("matchesQuery", () => {
  it("needs every word, in any order", () => {
    expect(matchesQuery("new ssh", "File > New SSH session")).toBe(true);
    expect(matchesQuery("ssh new", "File > New SSH session")).toBe(true);
    expect(matchesQuery("docker", "File > New SSH session")).toBe(false);
    expect(matchesQuery("", "anything")).toBe(true);
  });
});
