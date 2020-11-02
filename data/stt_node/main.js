const fs = require('fs');
const Ds = require('deepspeech');

if (process.argv.length != 5) {
    console.log('node main.js [AUDIO] [MODEL] [SCORER]');
    process.exit();
}

let audio_file = process.argv[2];
let model_file = process.argv[3];
let scorer_file = process.argv[4];

let model = new Ds.Model(model_file);
model.enableExternalScorer(scorer_file);
let data = fs.readFileSync(audio_file)  // TODO: stream data?
console.log(model.stt(data));
