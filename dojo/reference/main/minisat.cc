// The SAT Dojo's MiniSat: what minisat/simp/Main.cc does with its default options (the
// simplifying solver: variable elimination, then search), reading DIMACS on stdin, answering in
// the SAT Competition format and writing a binary DRAT proof to the file named by the first
// argument (patches/minisat.patch adds the proof).

#include <stdio.h>
#include <stdlib.h>

#include "minisat/core/Dimacs.h"
#include "minisat/simp/SimpSolver.h"
#include "minisat/utils/System.h"

using namespace Minisat;

// minisat/utils/System.cc reads /proc and sets resource limits, which WASI doesn't have; only
// the statistics (verbosity > 0) use these
double Minisat::memUsed() { return 0; }
double Minisat::memUsedPeak(bool) { return 0; }

int main(int argc, char** argv)
{
    SimpSolver S;
    S.verbosity = 0;
    if (argc > 1 && (S.drat = fopen(argv[1], "wb")) == NULL)
        printf("c could not open the proof file %s\n", argv[1]);

    // Standard input, through the zlib.h next to this file
    parse_DIMACS(0, S, false);
    S.eliminate(true);
    lbool ret = l_False;
    if (S.okay()){
        vec<Lit> assumptions;
        ret = S.solveLimited(assumptions);
    }
    if (ret == l_False){
        vec<Lit> empty;
        S.dratAdd(empty);
    }
    if (S.drat != NULL) fclose(S.drat);

    if (ret == l_True){
        printf("s SATISFIABLE\n");
        for (int i = 0; i < S.nVars(); i++)
            printf("%s%s%d", i % 10 == 0 ? (i ? "\nv " : "v ") : " ", S.model[i] == l_False ? "-" : "", i + 1);
        printf("%s0\n", S.nVars() ? " " : "v ");
    }else
        printf(ret == l_False ? "s UNSATISFIABLE\n" : "s UNKNOWN\n");
    fflush(stdout);
    exit(ret == l_True ? 10 : ret == l_False ? 20 : 0);
}
