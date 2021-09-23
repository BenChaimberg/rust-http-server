#!/usr/bin/env perl

=head1 DESCRIPTION

uppercase — a CGI program that returns its input in uppercase letters

=cut
print "Content-Type: text/plain\n\n";

print STDERR "\$\$ hello from perl \$\$\n";

foreach my $line ( <STDIN> ) {
    chomp( $line );
    $upline = uc $line;
    printf "$upline\n";
}
