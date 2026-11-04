#!/usr/bin/env python3
"""Manage only this instance's native server, private dependencies and persistent volumes."""
from __future__ import annotations

import argparse
import pathlib

from manage_backup import backup, restore
from manage_frontdoor import down, frontdoor, stop, stop_frontdoor
from manage_runtime import (
    IMAGE_REFS, cluster, command, compose, init, instance, owned, pin, ready, require, up,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['init', 'pin', 'up', 'status', 'drain', 'restart', 'down', 'backup', 'restore', 'cluster', 'frontdoor'])
    parser.add_argument('--instance', default='default')
    parser.add_argument('--port', type=int, default=8880)
    parser.add_argument('--nodes', type=int, choices=[1, 2], default=1)
    parser.add_argument('--relay-image')
    parser.add_argument('--fdb-image')
    parser.add_argument('--backup', type=pathlib.Path)
    parser.add_argument('--frontdoor-port', type=int)
    parser.add_argument('--disable', action='store_true', help='Disable only the optional frontdoor')
    args = parser.parse_args()
    require(not args.disable or args.action == 'frontdoor', '--disable applies only to frontdoor')
    require(args.frontdoor_port is None or args.action in ['frontdoor', 'restore'], 'Frontdoor port applies only to frontdoor or restore')
    if args.action == 'init':
        init(args.instance, args.port, args.nodes, args.relay_image or IMAGE_REFS['relay'], args.fdb_image or IMAGE_REFS['fdb'])
        return
    if args.action == 'restore':
        require(args.backup is not None, 'Specify --backup')
        require(not args.disable, '--disable applies only to frontdoor')
        restore(args.backup, args.instance, args.port, args.frontdoor_port)
        return
    path, owner = instance(args.instance)
    if args.action == 'frontdoor':
        require(not args.disable or args.frontdoor_port is None, 'Disable does not change the frontdoor port')
        frontdoor(path, owner, args.frontdoor_port, args.disable)
        return
    require(not args.disable and args.frontdoor_port is None, 'Frontdoor options apply only to frontdoor or restore')
    owned(path, owner)
    if args.action == 'pin':
        pin(path, owner, args.relay_image, args.fdb_image)
    elif args.action == 'up':
        up(path, owner)
    elif args.action == 'status':
        command(compose(path, owner) + ['ps'], capture=False)
        ready(owner, 3)
        print('Readiness passed')
    elif args.action == 'drain':
        stop_frontdoor(path, owner)
        command(compose(path, owner) + ['stop', '--timeout', '45', 'relay'] + (['relay2'] if owner['nodes'] == 2 else []), capture=False, timeout=100)
    elif args.action == 'down':
        down(path, owner)
    elif args.action == 'restart':
        stop(path, owner)
        up(path, owner)
    elif args.action == 'backup':
        backup(path, owner)
    elif args.action == 'cluster':
        cluster(path, owner)


if __name__ == '__main__':
    main()
