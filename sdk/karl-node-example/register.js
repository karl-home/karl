const karl = require("karl");
const CONTROLLER_PORT = "59582";
var CONTROLLER_IP;
if (process.argv.length > 2) {
  CONTROLLER_IP = process.argv[2];
} else {
  CONTROLLER_IP = "192.168.1.5";
}
const CLIENT_ID = "almond";

async function main() {
  await karl
    .registerClient(CONTROLLER_PORT, CONTROLLER_IP, CLIENT_ID)
    .then(function(result) {
      console.log(`export KARL_CLIENT_TOKEN=${result.getClientToken()}`);
      console.error("execute the export command before running stt_client.js");
    });
}

console.time("registerClient");
main();
console.timeEnd("registerClient");
