// The SAT Dojo's CaDiCaL: its own command line (src/cadical.cpp, built with its main renamed
// to cadical_main) run as `cadical -q --lrat - PROOF`. It reads DIMACS on stdin, answers in
// the SAT Competition format and writes a binary LRAT proof to the file named by the first
// argument, which `c proof lrat` announces to the Dojo.

#include <cstdio>

int cadical_main (int argc, char **argv);

int main (int argc, char **argv) {
  char name[] = "cadical", quiet[] = "-q", lrat[] = "--lrat", input[] = "-";
  char *args[] = {name, quiet, input, 0, 0, 0};
  int count = 3;
  if (argc > 1) {
    std::printf ("c proof lrat\n");
    std::fflush (stdout);
    args[count++] = lrat;
    args[count++] = argv[1];
  }
  return cadical_main (count, args);
}
