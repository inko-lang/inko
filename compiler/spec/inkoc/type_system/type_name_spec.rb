# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::TypeSystem::TypeName do
  describe '#type_name' do
    context 'when a list of type parameters without requirements is defined' do
      it 'returns the type name' do
        object = Inkoc::TypeSystem::Object.new(name: 'Foo')

        object.define_type_parameter('A', [])
        object.define_type_parameter('B', [])

        expect(object.type_name).to eq('Foo!(A, B)')
      end
    end

    context 'when a list of type parameters with requirements is defined' do
      it 'returns the type name' do
        object = Inkoc::TypeSystem::Object.new(name: 'Foo')
        trait1 = Inkoc::TypeSystem::Trait.new(name: 'T1')
        trait2 = Inkoc::TypeSystem::Trait.new(name: 'T2')

        object.define_type_parameter('A', [trait1])
        object.define_type_parameter('B', [trait1, trait2])

        expect(object.type_name).to eq('Foo!(T1, T1 + T2)')
      end
    end

    context 'when a list of initialised type parameters is defined' do
      it 'returns the type name' do
        object = Inkoc::TypeSystem::Object.new(name: 'Foo')
        instance = Inkoc::TypeSystem::Object.new(name: 'Integer')
        param = object.define_type_parameter('A', [])

        object.initialize_type_parameter(param, instance)

        expect(object.type_name).to eq('Foo!(Integer)')
      end
    end
  end
end
