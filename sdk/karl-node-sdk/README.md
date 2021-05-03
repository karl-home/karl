# karl-node-sdk

```
const karl = require("karl");
const CONTROLLER_PORT = "59582";
const CONTROLLER_IP = "192.168.1.3";
const AUDIO_FILE = "data/stt_node/weather.wav";

function buildRequest(audioFile) {
  let builder = new karl.ComputeRequestBuilder("node");
  builder.args(["main.js", audioFile, "models.pbmm", "models.scorer"]);
  builder.import("stt_node");
  builder.add_file(audioFile);
  let request = builder.finalize();
  request.setStdout(true);
  request.setStderr(true);
  return request;
}

async function main() {
  let host = await karl.getHost(CONTROLLER_PORT, CONTROLLER_IP);
  let request = buildRequest(AUDIO_FILE);
  let result = await karl.sendCompute(host.getPort(), host.getIp(), request);
  let stdout = new Buffer(result.getStdout()).toString('ascii');
  let stderr = new Buffer(result.getStderr()).toString('ascii');
  console.log('stdout:', stdout);
  console.log('stderr:', stderr);
}

main();
```
