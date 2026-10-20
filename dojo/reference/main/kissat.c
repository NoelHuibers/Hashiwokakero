// The SAT Dojo's Kissat: its own command line (src/main.c, built with its main renamed to
// kissat_main) run as `kissat -q - PROOF`. It reads DIMACS on stdin, answers in the SAT
// Competition format and writes a binary DRAT proof to the file named by the first argument.

int kissat_main (int argc, char **argv);

int main (int argc, char **argv) {
  char name[] = "kissat", quiet[] = "-q", input[] = "-";
  char *args[] = {name, quiet, input, argc > 1 ? argv[1] : 0, 0};
  return kissat_main (argc > 1 ? 4 : 3, args);
}
