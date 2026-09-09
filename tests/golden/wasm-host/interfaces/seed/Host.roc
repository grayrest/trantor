Host := [].{
	## A host-supplied runtime value the compiler cannot const-fold, so the
	## app's string build hits roc_alloc for real (gate-zero's reasoning).
	seed! : {} => I64
}
