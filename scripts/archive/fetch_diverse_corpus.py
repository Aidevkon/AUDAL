#!/usr/bin/env python3
import urllib.request
import json
import urllib.parse
import os
import subprocess

def download_category(category_name, queries, target_dir, num_files_per_query=6):
    os.makedirs(target_dir, exist_ok=True)
    count = 0
    for q in queries:
        query_encoded = urllib.parse.quote_plus(q + ' AND mediatype:audio')
        url = f"https://archive.org/advancedsearch.php?q={query_encoded}&fl[]=identifier&sort[]=downloads+desc&rows=5&page=1&output=json"
        
        try:
            req = urllib.request.Request(url)
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read())
                
            q_count = 0
            for doc in data["response"]["docs"]:
                if q_count >= num_files_per_query: break
                ident = doc["identifier"]
                meta_url = f"https://archive.org/metadata/{ident}"
                
                with urllib.request.urlopen(meta_url) as meta_resp:
                    meta_data = json.loads(meta_resp.read())
                    
                for f in meta_data.get("files", []):
                    name = f.get("name", "")
                    if name.endswith(".mp3") or name.endswith(".flac"):
                        print(f"[{category_name}] Extracting 30s of {name} from {ident}...")
                        dl_url = f"https://archive.org/download/{ident}/{urllib.parse.quote(name)}"
                        wav_path = os.path.join(target_dir, f"{category_name}_{count}.wav")
                        
                        try:
                            # Stream directly with ffmpeg for 30s only
                            subprocess.run([
                                "ffmpeg", "-i", dl_url, "-t", "30", "-ar", "48000", "-ac", "2", "-c:a", "pcm_s16le", wav_path, "-y", "-loglevel", "error"
                            ], check=True)
                            
                            count += 1
                            q_count += 1
                            if q_count >= num_files_per_query: break
                        except Exception as e:
                            print(f"  Failed ffmpeg: {e}")
        except Exception as e:
            print(f"Failed query {q}: {e}")

music_queries = [
    'subject:"classical" OR subject:"symphony"',
    'subject:"techno" OR subject:"edm"',
    'subject:"jazz"',
    'subject:"ambient" OR subject:"drone"',
    'subject:"metal" OR subject:"rock"'
]

speech_queries = [
    'subject:"spanish" AND subject:"speech"',
    'subject:"french" AND subject:"speech"',
    'subject:"podcast" AND subject:"interview"',
    'subject:"news" AND subject:"broadcast"',
    'subject:"audiobook" AND subject:"librivox"'
]

print("Fetching DIVERSE MUSIC...")
download_category("music", music_queries, "/tmp/diverse_corpus/music", num_files_per_query=6)

print("Fetching DIVERSE SPEECH...")
download_category("speech", speech_queries, "/tmp/diverse_corpus/speech", num_files_per_query=6)

print("Diverse corpus generation complete!")
