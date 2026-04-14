import asyncio
import time
import sys
import os

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), 'skills')))

from proactive.proactive import check_cve_alerts, ClawHubClient

class BenchClawHubClient:
    async def get_cves(self, skill_name: str):
        await asyncio.sleep(0.01) # Simulate network latency
        return [{"cve_id": f"CVE-{skill_name}", "severity": "HIGH", "description": "test"}]

    async def get_batch_cves(self, skill_names: list[str]):
        await asyncio.sleep(0.01) # One API call for all skills
        return {name: [{"cve_id": f"CVE-{name}", "severity": "HIGH", "description": "test"}] for name in skill_names}

async def main():
    client = BenchClawHubClient()
    skills = [f"skill_{i}" for i in range(100)]

    start = time.perf_counter()
    await check_cve_alerts(skills, client)
    end = time.perf_counter()
    print(f"Time taken for 100 skills: {end - start:.4f} seconds")

if __name__ == "__main__":
    asyncio.run(main())
