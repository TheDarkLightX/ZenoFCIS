# Tutorial: write and check a `.zeno` project

Start with the checked project already in the repository:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  check examples/minimal/project.zeno
```

The command prints one stable summary:

```text
checked examples/minimal/project.zeno: project=1 components=1 claims=1 unresolved_obligations=2 semantic_program_hash=57385a54387db8ad3e2da9a46a9ae22d2e72502b336120f570791d79e736b365
```

The source is small enough to read in one view:

```zeno
zeno 1;
project 1 minimal;
namespace 10 core;
type 100 state State;
type 101 command Command;
type 102 context Context;
type 103 destination Destination;
type 104 payload Payload;
reason 200 invalid precedence 0;
component 300 machine {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 1024;
}
merge [300];
law 400 identity = pre.100 == pre.100;
claim 500 identity cvc5 relational = pre.100 == pre.100;
```

Every important name has an explicit number. Those numbers stay stable when a
human changes comments or rearranges declarations. The merge list keeps its
written order because it decides which component has precedence.

The long hash identifies the checked program meaning. It is useful for review,
generated-file checks, retained evidence, and replay. A changed stable ID,
connection, rule, claim, or merge order changes that identity.

## See several mistakes in one run

Run the teaching example with three independent mistakes:

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  check examples/diagnostics-tour/project.zeno --format human
```

```text
ZENO-E0205 elaborate:1:1 composition.merge_order expected exact component-ID permutation; got [20, 21]; list every component exactly once in semantic merge order
ZENO-E0201 elaborate:6:1 type.id expected unique stable ID; got 10; allocate a distinct explicit ID
ZENO-E0203 elaborate:7:1 component.20.owns expected declared type ID; got 99; declare the referenced type
```

![One bounded check reports three authoring problems](../assets/marketing/terminal-accumulated-diagnostics.png)

The compiler keeps checking after the first independent problem. Each message
contains a stable code, source location, observed value, expected value, and a
suggested repair. The order remains stable, which makes the JSON form suitable
for editors and continuous integration.

## Create a fresh project

```bash
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  new /tmp/zeno-minimal --template minimal
cargo +1.97.1 run --quiet -p zeno-fcis-cli --locked -- \
  check /tmp/zeno-minimal/project.zeno
```

`new` refuses a target that already contains files. A `.zeno` file cannot
import files, read environment variables, select executable paths, or run
commands. These limits keep the authoring input small and reviewable.

The [language reference](../ZENO_LANGUAGE_V1.md) defines every declaration,
limit, and error rule. The [CLI reference](../CLI_REFERENCE.md) defines JSON
fields and exit codes.
