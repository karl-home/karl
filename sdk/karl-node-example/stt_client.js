const karl = require("karl");
const CONTROLLER_PORT = "59582";
var CONTROLLER_IP;
if (process.argv.length > 2) {
  CONTROLLER_IP = process.argv[2];
} else {
  CONTROLLER_IP = "192.168.1.5";
}
const CLIENT_ID = "almond";
const CLIENT_TOKEN = process.env.KARL_CLIENT_TOKEN;
if (!CLIENT_TOKEN) {
  console.error("set KARL_CLIENT_TOKEN environment variable");
  process.exit();
}
const AUDIO_FILE = "data/stt_node/weather.wav";

function buildRequest(audioFile) {
  console.time("buildRequest");
  let builder = new karl.ComputeRequestBuilder("storage/bin/node");
  builder.args(["storage/main.js", audioFile, "storage/models.pbmm", "storage/models.scorer"]);
  builder.add_file(audioFile);
  let request = builder.finalize();
  request.setStdout(true);
  request.setStderr(true);
  request.setStorage(true);
  request.setClientId(CLIENT_ID);
  console.timeEnd("buildRequest");
  return request;
}

async function main(request) {
  let host = await karl.getHost(CONTROLLER_PORT, CONTROLLER_IP);
  let result = await karl.sendCompute(host.getPort(), host.getIp(), request);
  let stdout = Buffer.from(result.getStdout()).toString('ascii');
  let stderr = Buffer.from(result.getStderr()).toString('ascii');
  console.log('stdout:', stdout);
  console.log('stderr:', stderr);
}

async function mainSync(request) {
  console.time("sendComputeBlocking");
  var stdout;
  var stderr;
  await karl
    .sendComputeBlocking(CONTROLLER_PORT, CONTROLLER_IP, CLIENT_TOKEN, request)
    .then(function(result) {
      stdout = Buffer.from(result.getStdout()).toString('ascii');
      stderr = Buffer.from(result.getStderr()).toString('ascii');
    })
    .catch(function (e) { throw e; });
  console.timeEnd("sendComputeBlocking");
  console.timeEnd("total");
  console.log('stdout:', stdout);
  console.log('stderr:', stderr);
}

console.time("total");
mainSync(buildRequest(AUDIO_FILE));
