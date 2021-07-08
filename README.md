# Lagoon

Lagoon is a thread pool crate that aims to address many of the problems with existing thread pool crates.

## Features

- **Scoped jobs**: Safely spawn jobs that have access to their parent scope!
- **Job handles**: Receive the result of a job when it finishes, or wait on it to finish!
- **Global pool**: A pay-for-what-you-use global thread pool that avoids dependencies fighting machine resources!
- **Customise thread attributes**: Specify thread name, stack size, etc.

## Planned Features

- **Async support for job waiting**: Use the thread pool in an async context!
