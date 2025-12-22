import rerun as rr
import math

def main():
    rr.init("rerun_example_any_values", spawn=True)
    for sample in range(1000):
        rr.log(
            "any_values",
            rr.AnyValues()
                .with_component_override(":sin", rr.components.ScalarBatch._COMPONENT_TYPE, [math.sin(sample * .0628)])
                .with_component_override(":cos", rr.components.ScalarBatch._COMPONENT_TYPE, [math.cos(sample * .0628)])
        )

        if sample % 200 == 0:
            rr.log(
                "any_values",
                rr.AnyValues()
                .with_component_override(":mode", rr.components.TextBatch._COMPONENT_TYPE, f"Mode {sample // 200}")
            )


if __name__ == "__main__":
    main()
