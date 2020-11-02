const fs = require('fs');
const Ds = require('deepspeech');

if (process.argv.length != 3) {
    console.log('node main.js [AUDIO]');
    process.exit();
}

let model_file = 'models.pbmm';
let scorer_file = 'models.scorer';
let audio_file = process.argv[2];

let model = new Ds.Model(model_file);
model.enableExternalScorer(scorer_file);
let data = fs.readFileSync(audio_file)  // TODO: stream data?
console.log(model.stt(data));
