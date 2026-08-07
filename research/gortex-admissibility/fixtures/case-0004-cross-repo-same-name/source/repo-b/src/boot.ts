import { Config } from "./config";

export function bootB(): Config {
  return Config.load("b.toml");
}
