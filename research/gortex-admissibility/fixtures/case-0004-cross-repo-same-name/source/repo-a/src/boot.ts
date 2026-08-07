import { Config } from "./config";

export function bootA(): Config {
  return Config.load("a.toml");
}
