# sbir_award_keyword_search.py

import requests

API_URL = "https://api.www.sbir.gov/public/api/awards"
AGENCY = "DOD"  # Change to DOE, NASA, etc. if needed
ROWS = 200  # Up to 1000 per call if needed
MAX_PAGES = 5  # Increase to fetch more pages

KEYWORDS = [
    "legacy", "modernization", "64-bit", "corba", "porting",
    "firmware", "toolchain", "ada", "rtos", "reverse engineering"
]

def fetch_awards(page):
    offset = page * ROWS
    params = {
        "agency": AGENCY,
        "rows": ROWS,
        "start": offset,
    }

    try:
        r = requests.get(API_URL, params=params)
        r.raise_for_status()
        return r.json()
    except Exception as e:
        print(f"❌ Error fetching data: {e}")
        return []

def search_awards(data, keywords):
    matches = []
    for award in data:
        text = f"{award.get('award_title', '')} {award.get('abstract', '')} {award.get('research_area_keywords', '')}".lower()
        for kw in keywords:
            if kw.lower() in text:
                matches.append({
                    "keyword": kw,
                    "title": award.get("award_title", ""),
                    "firm": award.get("firm", ""),
                    "year": award.get("award_year", ""),
                    "abstract": award.get("abstract", ""),
                    "link": award.get("award_link", "")
                })
                break
    return matches

if __name__ == "__main__":
    print(f"🔍 Searching SBIR Awards for agency: {AGENCY}")
    all_matches = []

    for page in range(MAX_PAGES):
        print(f"📄 Fetching page {page + 1}")
        data = fetch_awards(page)
        matches = search_awards(data, KEYWORDS)
        all_matches.extend(matches)

    if all_matches:
        print(f"\n✅ Found {len(all_matches)} matches:\n")
        for match in all_matches:
            print(f"[{match['year']}] {match['title']} — {match['firm']}")
            print(f"Keyword: {match['keyword']}")
            print(f"Link: {match['link']}")
            print(f"Abstract: {match['abstract'][:150]}...\n")
    else:
        print("❌ No keyword matches found.")
