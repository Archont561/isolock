Feature: Clifacet application routing

  Scenario: Unknown commands fail before application setup
    Given an isolated CLI workspace
    When an unknown command is requested
    Then a usage error is reported on stderr
    And no application state is created

  Scenario: A missing command shows root usage
    Given an isolated CLI workspace
    When no command is requested
    Then a usage error mentioning "Usage: isolock" is reported on stderr
    And no application state is created

  Scenario Outline: Invalid arguments fail before application setup
    Given an isolated CLI workspace
    And project configuration is invalid
    When CLI arguments "<arguments>" are requested
    Then a usage error mentioning "<diagnostic>" is reported on stderr
    And no application state is created

    Examples:
      | arguments                 | diagnostic              |
      | plan --unknown            | unexpected argument     |
      | serve --unknown           | unexpected argument     |
      | config                    | Usage: isolock config   |
      | config unknown-command    | unrecognized subcommand |
      | config explain            | <PATH>                  |
      | exec                      | <COMMAND>               |
      | exec --                   | <COMMAND>               |
      | export                    | --output                |
      | ingest --registry invalid | invalid value           |
      | --config                  | a value is required     |

  Scenario Outline: Help does not initialize the application
    Given an isolated CLI workspace
    And project configuration is invalid
    When CLI arguments "<arguments>" are requested
    Then generated help is reported on stdout
    And no application state is created

    Examples:
      | arguments             |
      | --help                |
      | help                  |
      | plan --help           |
      | serve --help          |
      | exec --help           |
      | config --help         |
      | config explain --help |
      | bootstrap --help      |
      | fetch --help          |
      | sync --help           |
      | export --help         |
      | seed-manifest --help  |
      | gc --help             |
      | ingest --help         |

  Scenario Outline: Child arguments are opaque to the application parser
    Given an isolated CLI workspace
    When a child receives application-looking arguments "<separator>"
    Then its complete argument tail is passed through unchanged

    Examples:
      | separator |
      | directly  |
      | after --  |

  Scenario Outline: Child exit statuses are not collapsed into application errors
    Given an isolated CLI workspace
    When a child exits with status <status>
    Then exit status <status> and both child streams are preserved

    Examples:
      | status |
      | 0      |
      | 2      |
      | 9      |
      | 42     |
      | 125    |
      | 127    |
      | 255    |

  Scenario Outline: Global overrides reach nested commands
    Given an isolated CLI workspace
    When CLI arguments "<arguments>" are requested
    Then the nested command reports the explicit proxy port
    And no application state is created

    Examples:
      | arguments                             |
      | --port 8123 config explain proxy.port |
      | config --port 8123 explain proxy.port |
      | config explain proxy.port --port 8123 |

  Scenario: Legacy ingestion keeps its workflow arguments
    Given an isolated CLI workspace
    When CLI arguments "ingest --registry cargo demo==1.0.0" are requested
    Then the legacy ingestion entrypoint succeeds
    And no application state is created

  Scenario: Version does not initialize the application
    Given an isolated CLI workspace
    And project configuration is invalid
    When CLI arguments "config check --version" are requested
    Then the application version succeeds on stdout
    And no application state is created
