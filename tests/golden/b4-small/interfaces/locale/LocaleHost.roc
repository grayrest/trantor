LocaleHost :: [].{
	get! : {} => Try(Str, [NotAvailable])
	count! : {} => U64
	at! : U64 => Str
}
