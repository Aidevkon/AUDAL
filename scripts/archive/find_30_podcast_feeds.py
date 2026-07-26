#!/usr/bin/env python3
import urllib.request
import json
import re
import socket

socket.setdefaulttimeout(2.5)

candidate_feeds = [
    # --- ENGLISH ---
    ("NPR News Now", "https://feeds.npr.org/500005/podcast.xml", "English", "News Briefing", "Fast"),
    ("BBC Global News Podcast", "https://podcasts.files.bbci.co.uk/p02nq0gn.rss", "English", "News / Multi-host", "Fast"),
    ("TED Radio Hour", "https://feeds.npr.org/510298/podcast.xml", "English", "Multi-speaker / Produced", "Medium"),
    ("Planet Money", "https://feeds.npr.org/510289/podcast.xml", "English", "Storytelling / Produced", "Medium"),
    ("Fresh Air (NPR)", "https://feeds.npr.org/381444908/podcast.xml", "English", "Interview / Solo Host", "Slow-Medium"),
    ("Short Wave (NPR)", "https://feeds.npr.org/510351/podcast.xml", "English", "Science / Dialogue", "Fast"),
    ("BBC In Our Time", "https://podcasts.files.bbci.co.uk/b006qykl.rss", "English", "Panel Discussion", "Medium"),
    ("BBC Minute", "https://podcasts.files.bbci.co.uk/p02pc9ny.rss", "English", "Ultra-Fast Briefing", "Very Fast"),
    ("BBC World Business Report", "https://podcasts.files.bbci.co.uk/p02nr07g.rss", "English", "Business / Interview", "Fast"),
    ("Vox Today Explained", "https://feeds.megaphone.fm/VMP5705694065", "English", "Narrative / Dialogue", "Medium"),
    ("Science Vs", "https://feeds.megaphone.fm/sciencevs", "English", "Humorous Science Dialogue", "Fast-Medium"),
    ("Throughline (NPR)", "https://feeds.npr.org/510333/podcast.xml", "English", "History / Produced", "Slow-Medium"),
    ("Wait Wait Don't Tell Me", "https://feeds.npr.org/510208/podcast.xml", "English", "Comedy Panel / Live", "Fast"),
    ("Up First (NPR)", "https://feeds.npr.org/510318/podcast.xml", "English", "Daily Briefing", "Fast"),
    ("Code Switch (NPR)", "https://feeds.npr.org/510312/podcast.xml", "English", "Culture / Dialogue", "Medium"),
    ("Consider This (NPR)", "https://feeds.npr.org/510355/podcast.xml", "English", "Evening News", "Fast"),
    ("Life Kit (NPR)", "https://feeds.npr.org/510338/podcast.xml", "English", "Advice / Interview", "Medium"),
    ("Embedded (NPR)", "https://feeds.npr.org/510311/podcast.xml", "English", "Documentary / Deep Dive", "Slow-Medium"),

    # --- GERMAN ---
    ("DW Langsam gesprochene Nachrichten", "https://rss.dw.com/xml/DKpodcast_lgn_de", "German", "Solo News (Slow)", "Slow"),
    ("Deutschlandfunk Nachrichten", "https://www.deutschlandfunk.de/podcast-nachrichten.1257.de.podcast.xml", "German", "News Briefing", "Fast"),

    # --- FRENCH ---
    ("France Culture - Les Chemins de la philosophie", "https://radiofrance-podcast.net/podcast09/rss_10351.xml", "French", "Solo / Discussion", "Medium"),

    # --- SPANISH ---
    ("Radio Ambulante", "https://feeds.npr.org/510315/podcast.xml", "Spanish", "Narrative / Storytelling", "Medium"),

    # --- SWEDISH / ARABIC ---
    ("Sveriges Radio Ekot", "https://api.sr.se/api/rss/pod/3795", "Swedish", "News Briefing", "Fast"),
    ("Sveriges Radio P1 Dokumentär", "https://api.sr.se/api/rss/pod/2422", "Swedish", "Documentary", "Slow-Medium"),
    ("Sveriges Radio P3 Nyheter", "https://api.sr.se/api/rss/pod/4143", "Swedish", "Youth News", "Fast"),
    ("BBC Arabic Podcast", "https://podcasts.files.bbci.co.uk/p02pc9qx.rss", "Arabic", "News / Feature", "Medium"),

    # --- MORE NPR / BBC ---
    ("Pop Culture Happy Hour (NPR)", "https://feeds.npr.org/510282/podcast.xml", "English", "Pop Culture Roundtable", "Fast"),
    ("All Songs Considered (NPR Spoken)", "https://feeds.npr.org/510019/podcast.xml", "English", "Music Review / Dialogue", "Medium"),
    ("BBC The Document", "https://podcasts.files.bbci.co.uk/p02nq0lx.rss", "English", "Archive Documentary", "Slow-Medium"),
    ("BBC Radio 4 Drama", "https://podcasts.files.bbci.co.uk/p02pc9q6.rss", "English", "Audio Drama Monologue", "Slow-Medium"),
    ("BBC Science In Action", "https://podcasts.files.bbci.co.uk/p02nq0dn.rss", "English", "Global Science", "Fast"),
    ("BBC More or Less", "https://podcasts.files.bbci.co.uk/p02nq0d4.rss", "English", "Statistics / Analysis", "Medium"),
    ("BBC The Inquiry", "https://podcasts.files.bbci.co.uk/p02nr08x.rss", "English", "Investigative / Expert", "Medium"),
    ("BBC Discovery", "https://podcasts.files.bbci.co.uk/p02nq0ks.rss", "English", "Science Documentary", "Medium"),
    ("NPR Politics Podcast", "https://feeds.npr.org/510310/podcast.xml", "English", "Political Dialogue", "Fast")
]

verified = []
headers = {'User-Agent': 'Mozilla/5.0'}

for name, url, lang, vtype, rate in candidate_feeds:
    if len(verified) >= 30:
        break
    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as resp:
            content = resp.read().decode('utf-8', errors='ignore')
            m = re.search(r'<enclosure[^>]+url=["\']([^"\']+)["\']', content, re.IGNORECASE)
            if m:
                audio_url = m.group(1)
                m_title = re.search(r'<item>.*?<title>(.*?)</title>', content, re.DOTALL | re.IGNORECASE)
                ep_title = m_title.group(1).strip() if m_title else "Episode"
                ep_title = re.sub(r'<!\[CDATA\[(.*?)\]\]>', r'\1', ep_title)
                verified.append({
                    "show_name": name,
                    "feed_url": url,
                    "language": lang,
                    "voice_type": vtype,
                    "speaking_rate": rate,
                    "ep_title": ep_title,
                    "audio_url": audio_url
                })
                print(f"[{len(verified):2d}/30] OK: {name:<35} | {lang:<10} | {vtype}")
    except Exception:
        pass

print(f"\nTotal Verified Podcast Feeds: {len(verified)}")
with open("/tmp/podcast_30_feeds.json", "w") as f:
    json.dump(verified, f, indent=2)
