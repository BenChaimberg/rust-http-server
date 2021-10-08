#!/usr/bin/perl

$company = $ENV{'QUERY_STRING'};
print "Content-Type: text/html\n";
print "\n";

if ($company =~ /appl/) {
  my $var_rand = rand();
  print 450 + 10 * $var_rand;
} else {
  print "150";
}
