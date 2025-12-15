import dataclasses
import datetime
import enum
import json
import os
import re

import collections
import numpy as np


class EventType(enum.Enum):
    START_PROPOSAL = "Start building proposal"
    START_VALIDATOR = "Start validating proposal"
    FINISH_PROPOSAL = "Finished building proposal"
    FINISH_VALIDATOR = "Finished validating proposal"


@dataclasses.dataclass(frozen=True)
class Event:
    timestamp: datetime.datetime
    namespace: str
    event_type: EventType
    height: int
    round: int


class Processor:
    def __init__(self, filename):
        self.events = self.load_data(filename)

    def analyze_validate_duration(self):
        results = collections.defaultdict(list)

        for event in self.events:
            if event.event_type == EventType.START_PROPOSAL:
                es = event
            elif event.event_type == EventType.START_VALIDATOR:
                if event.height != es.height or event.round != es.round:
                    continue
                # d = (event.timestamp - es.timestamp).total_seconds()
                # if d > 1:
                #    print(f"Long delay between proposal start and validator start: {d:.3f} sec at height {event.height}, round {event.round}, namespace {event.namespace}")
            elif event.event_type == EventType.FINISH_PROPOSAL:
                ee = event
            elif event.event_type == EventType.FINISH_VALIDATOR:
                if event.height != ee.height or event.round != ee.round:
                    continue

                d = (event.timestamp - ee.timestamp).total_seconds()
                results[event.namespace].append((d, event))

        for namespace, durations in sorted(results.items()):
            arr = np.array([d[0] for d in durations])
            print(f"Namespace: {namespace}")
            print(f"  Count: {len(arr)}")
            print(f"  Mean: {np.mean(arr):.3f} sec")
            print(f"  Median: {np.median(arr):.3f} sec")
            print(f"  95th percentile: {np.percentile(arr, 95):.3f} sec")
            print(f"  99th percentile: {np.percentile(arr, 99):.3f} sec")
            print(f"  Max: {np.max(arr):.3f} sec")

            # print all events with duration > 1 sec
            for duration, event in durations:
                if duration > 1:
                    print(
                        f"  Event: {event.namespace}, height: {event.height}, round: {event.round}, Duration: {duration:.3f} sec"
                    )

    def analyze_start_validation_delay(self):
        start_proposal_times = collections.defaultdict(datetime.datetime)
        start_validation_times = collections.defaultdict(list)

        for event in self.events:
            if event.event_type == EventType.START_PROPOSAL:
                start_proposal_times[(event.height, event.round)] = event.timestamp
            elif event.event_type == EventType.START_VALIDATOR:
                key = (event.height, event.round)
                st = start_proposal_times.get(key)
                if st:
                    d = (event.timestamp - st).total_seconds()
                    start_validation_times[event.namespace].append(d)

        for namespace, delays in sorted(start_validation_times.items()):
            arr = np.array(delays)
            print(f"Start Validation Delays - Namespace: {namespace}")
            # print(f"  Count: {len(arr)}")
            print(f"  Median: {np.median(arr):.3f} sec")
            print(f"  95th percentile: {np.percentile(arr, 95):.3f} sec")
            print(f"  99th percentile: {np.percentile(arr, 99):.3f} sec")
            print(f"  Max: {np.max(arr):.3f} sec")

    def analyze_exec_duration(self):
        exec_start_times = collections.defaultdict(datetime.datetime)
        exec_times = collections.defaultdict(list)

        for event in self.events:
            if event.event_type in {EventType.START_VALIDATOR, EventType.START_VALIDATOR}:
                exec_start_times[(event.namespace, event.height, event.round)] = event.timestamp
            elif event.event_type in {EventType.FINISH_PROPOSAL, EventType.FINISH_VALIDATOR}:
                key = (event.namespace, event.height, event.round)
                st = exec_start_times.get(key)
                if st:
                    d = (event.timestamp - st).total_seconds()
                    exec_times[event.namespace].append(d)

        for namespace, durations in sorted(exec_times.items()):
            arr = np.array(durations)
            print(f"Execution Times - Namespace: {namespace}")
            print(f"  Count: {len(arr)}")
            print(f"  Median: {np.median(arr):.3f} sec")
            print(f"  99th percentile: {np.percentile(arr, 99):.3f} sec")
            print(f"  Max: {np.max(arr):.3f} sec")

    def load_data(self, filename):
        def find_in_spans(spans, key):
            for span in spans:
                if key in span:
                    return span[key]
            return None

        def extract_height_round_from_proposal_init(msg):
            m = re.search(r"ProposalInit { height: BlockNumber\((\d+)\), round: (\d+)", msg)
            if m:
                return m.group(1), int(m.group(2))
            return None, None

        with open(filename, "r") as file:
            data = json.load(file)
        ret = []
        for record in data:
            # Format is '2025-11-22T11:32:35.580749Z'
            ts = datetime.datetime.fromisoformat(record["timestamp"].replace("Z", "+00:00"))
            payload = json.loads(record["json_payload"])
            event_type = [et for et in EventType if et.value in payload["message"]][0]
            height = find_in_spans(payload["spans"], "height")
            round_ = find_in_spans(payload["spans"], "round")
            if round_ is None:
                if event_type is EventType.START_PROPOSAL:
                    h, round_ = extract_height_round_from_proposal_init(payload["proposal_init"])
                    assert h == height
                elif event_type is EventType.START_VALIDATOR:
                    round_ = payload["round"]

            ret.append(
                Event(
                    timestamp=ts,
                    namespace=record["namespace"],
                    event_type=event_type,
                    height=height,
                    round=round_,
                )
            )
        print(f"Loaded {len(ret)} events from {filename}")
        return ret


if __name__ == "__main__":
    dirname = "/home/matanl/Downloads/"
    filename = "downloaded-logs-20251122-143306.json"
    filename = "downloaded-logs-20251122-150926.json"

    p = Processor(os.path.join(dirname, filename))
    p.analyze_validate_duration()
    # p.analyze_exec_duration()
    # p.analyze_start_validation_delay()
