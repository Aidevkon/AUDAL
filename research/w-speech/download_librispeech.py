import urllib.request
import tarfile
import os

url = "http://www.openslr.org/resources/12/dev-clean.tar.gz"
dest_dir = "/home/aidevcon/Documents/creator-os/research/w-speech/corpus"
os.makedirs(dest_dir, exist_ok=True)

req = urllib.request.urlopen(url)
tar = tarfile.open(fileobj=req, mode="r|gz")
count = 0
limit = 30
try:
    for tarinfo in tar:
        if tarinfo.isreg() and tarinfo.name.endswith('.flac'):
            name = os.path.basename(tarinfo.name)
            f_out = tar.extractfile(tarinfo)
            with open(os.path.join(dest_dir, name), 'wb') as f:
                f.write(f_out.read())
            count += 1
            print(f"Extracted {name}")
            if count >= limit:
                break
except Exception as e:
    print(e)
print(f"Downloaded {count} files.")
