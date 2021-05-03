#!/usr/bin/env python3
import asyncio
import time
from google.cloud import speech_v1

def read_file():
    path = 'data/stt_node/weather.wav'
    f = open(path, 'rb')
    audio_bytes = f.read()
    print(f'audio file is {len(audio_bytes)} bytes')
    return audio_bytes

async def main(future):
    client = speech_v1.services.speech.SpeechAsyncClient()
    audio_bytes = read_file()
    audio = speech_v1.types.RecognitionAudio(content=audio_bytes)
    config = speech_v1.types.RecognitionConfig(language_code='en')
    response = await client.recognize(config=config, audio=audio)
    results = response.results
    future.set_result(results[0].alternatives[0].transcript)

if __name__ == '__main__':
    s = time.perf_counter()
    loop = asyncio.get_event_loop()
    future = asyncio.Future()
    asyncio.ensure_future(main(future))
    loop.run_until_complete(future)
    print(f'Top answer: {future.result()}')
    loop.close()
    elapsed = time.perf_counter() - s
    print(f'{__file__} executed in {elapsed:0.4f} seconds.')
