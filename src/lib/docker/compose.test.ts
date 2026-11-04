import { describe, expect, it } from "vitest";
import type { ContainerInfo } from "../backend/docker";
import { composeCommand, projectOf } from "./compose";

const box = (extra: Partial<ContainerInfo>): ContainerInfo => ({
  id: "a1",
  name: "shop-web-1",
  image: "nginx",
  state: "running",
  status: "Up",
  project: "shop",
  service: "web",
  ports: [],
  created: 0,
  ...extra,
});

describe("compose commands", () => {
  const containers = [box({ project_dir: "/srv/shop", config_files: "/srv/shop/compose.yaml,/srv/shop/extra.yaml" })];

  it("builds the command from the project's labels", () => {
    const p = projectOf("shop", containers)!;
    expect(composeCommand(p, "up")).toBe(
      "docker compose --project-directory '/srv/shop' -p 'shop' -f '/srv/shop/compose.yaml' -f '/srv/shop/extra.yaml' up -d",
    );
    expect(composeCommand(p, "logs")).toContain("logs -f --tail 200");
  });

  it("allows spaces and Windows paths", () => {
    const p = projectOf("shop", [box({ project_dir: "C:\Users\me\my shop", config_files: "C:\Users\me\my shop\compose.yaml" })])!;
    expect(composeCommand(p, "down")).toContain("'C:\Users\me\my shop'");
  });

  it("has nothing to run when the labels are missing", () => {
    expect(projectOf("shop", [box({})])).toBeNull();
    expect(projectOf("other", containers)).toBeNull();
  });

  it("refuses paths that could break out of the quotes", () => {
    for (const bad of ["/srv/it's", "/srv/a;rm -rf x", "/srv/$(id)", "/srv/a`b`", "/srv/a\nb", "/srv/\u2018x"]) {
      const p = { name: "shop", dir: bad, files: ["/srv/shop/compose.yaml"] };
      expect(composeCommand(p, "up")).toBeNull();
    }
    expect(composeCommand({ name: "x y", dir: "/srv", files: ["/srv/c.yaml"] }, "up")).toBeNull();
  });
});
