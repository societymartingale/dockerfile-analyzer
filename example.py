"""Usage: python example.py <dockerfile path>"""

from sys import argv
import dockerfile_analyzer as da


def get_dockerfile_contents(fname: str) -> str:
    with open(fname) as f:
        return f.read()


def main() -> None:
    df = get_dockerfile_contents(argv[1])
    res = da.analyze_dockerfile(df)
    print(res)


if __name__ == "__main__":
    main()
