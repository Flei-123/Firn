import asyncio

events = []


async def main():
    loop = asyncio.get_running_loop()
    t0 = loop.time()
    jobs = {}

    def every(name, ms):
        k = 0

        def fire():
            nonlocal k
            events.append(name)
            k += 1
            jobs[name] = loop.call_at(t0 + k * ms / 1000 + ms / 1000, fire)
        jobs[name] = loop.call_at(t0 + ms / 1000, fire)

    every("a", 50)
    every("b", 70)
    every("c", 110)
    loop.call_at(t0 + 0.175, lambda: events.append("o"))
    loop.call_at(t0 + 0.230, lambda: jobs["b"].cancel())
    await asyncio.sleep(0.385)
    for h in jobs.values():
        h.cancel()
    print(" ".join(events))
    print(" ".join(f"{n}={events.count(n)}" for n in "abco"))

asyncio.run(main())
