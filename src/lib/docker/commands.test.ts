import { describe, expect, it } from "vitest";
import { attachCommand, buildCommand, copyInCommand, copyOutCommand, loginCommand, pushCommand, tagCommand } from "./commands";

const cmd = (c: { command: string } | { error: string }) => ("command" in c ? c.command : "ERROR: " + c.error);

describe("docker commands", () => {
  it("builds the commands", () => {
    expect(cmd(buildCommand("myapp:1.0", "/srv/app", ""))).toBe("docker build -t 'myapp:1.0' '/srv/app'");
    expect(cmd(buildCommand("myapp", "C:\src\my app", "C:\src\my app\Dockerfile.dev"))).toBe(
      "docker build -t 'myapp' -f 'C:\src\my app\Dockerfile.dev' 'C:\src\my app'",
    );
    expect(cmd(tagCommand("app:1", "reg.example.com/team/app:1"))).toBe("docker tag 'app:1' 'reg.example.com/team/app:1'");
    expect(cmd(pushCommand("reg.example.com/team/app:1"))).toBe("docker push 'reg.example.com/team/app:1'");
    expect(cmd(loginCommand(""))).toBe("docker login");
    expect(cmd(loginCommand("reg.example.com:5000"))).toBe("docker login 'reg.example.com:5000'");
    expect(cmd(attachCommand("shop-web-1"))).toBe("docker attach --sig-proxy=false 'shop-web-1'");
    expect(cmd(copyOutCommand("web", "/etc/nginx/nginx.conf", "./nginx.conf"))).toBe("docker cp 'web:/etc/nginx/nginx.conf' './nginx.conf'");
    expect(cmd(copyInCommand("./a.txt", "web", "/tmp/"))).toBe("docker cp './a.txt' 'web:/tmp/'");
  });

  it("refuses anything that could break out of the quotes", () => {
    expect(cmd(buildCommand("x; rm -rf /", "/srv", ""))).toMatch(/^ERROR/);
    expect(cmd(buildCommand("x", "/srv/it's", ""))).toMatch(/^ERROR/);
    expect(cmd(buildCommand("x", "/srv", "a$(id)"))).toMatch(/^ERROR/);
    expect(cmd(tagCommand("a b", "c"))).toMatch(/^ERROR/);
    expect(cmd(pushCommand("-rf"))).toMatch(/^ERROR/);
    expect(cmd(loginCommand("host && calc"))).toMatch(/^ERROR/);
    expect(cmd(attachCommand("web; x"))).toMatch(/^ERROR/);
    expect(cmd(copyOutCommand("web", "/a\nb", "./x"))).toMatch(/^ERROR/);
    expect(cmd(copyInCommand("", "web", "/tmp"))).toMatch(/^ERROR/);
  });
});
