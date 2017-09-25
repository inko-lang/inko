# frozen_string_literal: true

module Inkoc
  module TypeLookup
    def type_for_constant(node, sources)
      name = node.name

      if node.receiver
        receiver = type_for_constant(node.receiver, sources)
        sources = [receiver] + sources
      end

      sources.each do |source|
        type = source.lookup_type(name)

        return type if type
      end

      diagnostics.undefined_constant_error(node.name, node.location)

      Type::Dynamic.new
    end
  end
end
