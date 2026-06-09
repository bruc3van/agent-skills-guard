// Example file system usage for documentation purposes
// This is a sample showing how fs.readFile works

const fs = require("fs");

/**
 * Example: reading a config file
 * This is only an example, not used in production.
 */
function exampleReadConfig() {
  // example: read a local config
  const data = fs.readFileSync("./config.json", "utf8");
  return JSON.parse(data);
}

module.exports = { exampleReadConfig };
