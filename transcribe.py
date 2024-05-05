import json
import threading
import time
from pydub import AudioSegment
import io
import asyncio
import aiofiles
from openai import OpenAI
import neuralspace as ns
import os

# initialize VoiceAI
vai = ns.VoiceAI(api_key="")

# Set up the transcription configuration
config = {
    "file_transcription": {
        "language_id": "ar",
        "mode": "advanced",
    },
    "summarize": True,
    "sentiment_detect": True,
    "translation": {
        "target_languages": [
            "en",
        ]
    }
}

client = OpenAI(
    # This is the default and can be omitted
    api_key=""
)


# Set the directory where the Rust script writes the audio chunks
audio_chunks_dir = './audio_chunks'
# directory to store temporary files
temp_dir = './tmp'

async def process_audio_chunk(file_path):
    async with aiofiles.open(file_path, 'rb') as file:
        audio_chunk = io.BytesIO(await file.read())
    
    # Submit a transcription job
    job_id = vai.transcribe(file=audio_chunk, config=config)
    print(f"Created job: {job_id}")
    
    # Wait for the job to complete
    result = vai.poll_until_complete(job_id)
    
    # Save the transcription result to a temporary file
    temp_file = os.path.join(temp_dir, f"{job_id}.json")
    async with aiofiles.open(temp_file, 'w') as file:
        await file.write(json.dumps(result))
    
    # Remove the processed audio chunk file
    await aiofiles.os.remove(file_path)
    print(f"Deleted file: {file_path}")

async def process_audio_chunks():
    chunk_files = [f for f in os.listdir(audio_chunks_dir) if f.endswith('.mp3')]

    tasks = []
    for chunk_file in chunk_files:
        file_path = os.path.join(audio_chunks_dir, chunk_file)
        task = asyncio.create_task(process_audio_chunk(file_path))
        tasks.append(task)

    await asyncio.gather(*tasks)

async def generate_composite_summary():
    # Read the temporary transcription files
    temp_files = [f for f in os.listdir(temp_dir) if f.endswith('.json')]
    
    transcriptions = []
    translations = []
    summaries = []
    for temp_file in temp_files:
        file_path = os.path.join(temp_dir, temp_file)
        async with aiofiles.open(file_path, 'r') as file:
            result = json.loads(await file.read())
            transcriptions.append(result['data']['result']['transcription']['transcript'])
            translations.append(result['data']['result']['translation']['en']['text'])
            summaries.append(result['data']['result']['transcription']['summary'])
    
    # Combine the transcriptions, translations, and summaries
    combined_transcription = '\n'.join(transcriptions)
    combined_translation = '\n'.join(translations)
    combined_summary = '\n'.join(summaries)
    
    # Make a ChatGPT API call to generate the composite summary
    openai_client = OpenAI(api_key=os.environ.get("OPENAI_API_KEY"))
    prompt = f"Please provide a composite summary of the following:\n\nTranscriptions:\n{combined_transcription}\n\nTranslations:\n{combined_translation}\n\nSummaries:\n{combined_summary}"
    chat_completion = openai_client.chat.completions.create(
        model="gpt-4-turbo-preview",
        messages=[
            {
                "role": "user",
                "content": prompt,
            },
        ],
    )
    composite_summary = chat_completion.choices[0].message.content.strip()
    
    print("Composite Summary:")
    print(composite_summary)
    # save the composite summary to a file
    async with aiofiles.open('./composite_summary.txt', 'w') as file:
        await file.write(composite_summary)
    
    # Clean up the temporary files
    for temp_file in temp_files:
        file_path = os.path.join(temp_dir, temp_file)
        await aiofiles.os.remove(file_path)

async def main():
    await process_audio_chunks()
    await generate_composite_summary()

asyncio.run(main())